use std::sync::{Arc, Mutex, RwLock, atomic::AtomicUsize};
use std::collections::{HashSet, HashMap};
use tokio::sync::mpsc;
use std::time::Instant;
use tokio::time::{self, Duration};
use crate::controller::{AuditLog, HostScheduler};
use crate::protos::ComputeRequest;
use karl_common::*;

#[derive(Debug)]
struct QueuedHook {
    id: HookID,
    /// Tag, timestamp, data
    trigger: Option<(String, String, Vec<u8>)>,
}

pub struct HookRunner {
    tx: Option<mpsc::Sender<QueuedHook>>,
    /// Registered hooks and their local hook IDs.
    pub(crate) hooks: Arc<Mutex<HashMap<HookID, Hook>>>,
    pub(crate) tag_counter: Arc<Mutex<AtomicUsize>>,
    /// Watched tags and the hooks they spawn.
    watched_tags: Arc<RwLock<HashMap<String, Vec<HookID>>>>,
}

/// Converts the hook to a protobuf compute request.
///
/// The caller must set the request token before sending the compute
/// reuqest to a host over the network.
fn hook_to_compute_request(
    hook: &Hook,
    host_token: HostToken,
    hook_id: String,
    cached: bool,
) -> Result<ComputeRequest, Error> {
    let package = if cached {
        vec![]
    } else {
        hook.package.clone()
    };
    let binary_path = hook.binary_path.clone().into_os_string().into_string().unwrap();
    let args = hook.args.clone().into_iter().collect();
    let envs = hook.envs.clone().iter().map(|(k, v)| format!("{}={}", k, v)).collect();
    let network_perm = hook.network_perm.clone().into_iter().collect();
    Ok(ComputeRequest {
        host_token,
        hook_id,
        cached,
        package,
        binary_path,
        args,
        envs,
        params: hook.params_string(),
        returns: hook.returns_string(),
        network_perm,
        triggered_tag: String::new(),
        triggered_timestamp: String::new(),
        triggered_data: Vec::new(),
    })
}

fn gen_process_id() -> ProcessID {
    use rand::Rng;
    rand::thread_rng().gen()
}

impl HookRunner {
    /// Create a new HookRunner.
    pub fn new() -> Self {
        Self {
            tx: None,
            hooks: Arc::new(Mutex::new(HashMap::new())),
            tag_counter: Arc::new(Mutex::new(AtomicUsize::new(0))),
            watched_tags: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn next_tag(&self) -> String {
        let mut tag_counter = self.tag_counter.lock().unwrap();
        let tag = tag_counter.get_mut();
        let old_tag = *tag;
        *tag = old_tag + 1;
        format!("t{}", old_tag)
    }

    /// Spawns queue manager to monitor the delay queue for when hooks
    /// are read to be scheduled. Add hooks to the queue via `queue_hook()`.
    pub fn start(
        &mut self,
        audit_log: Arc<Mutex<AuditLog>>,
        scheduler: Arc<Mutex<HostScheduler>>,
        mock_send_compute: bool,
    ) {
        let buffer = 100;  // TODO: tune
        let (tx, rx) = mpsc::channel::<QueuedHook>(buffer);
        self.tx = Some(tx);
        let hooks = self.hooks.clone();
        tokio::spawn(async move {
            Self::start_queue_manager(
                rx,
                hooks,
                audit_log,
                scheduler,
                mock_send_compute,
            ).await;
        });
    }

    /// Register a hook.
    ///
    /// Parameters:
    /// - global_hook_id - The ID of the hook from the global hook repository.
    ///
    /// IoError if error importing hook from filesystem.
    /// HookInstallError if environment variables are formatted incorrectly.
    pub fn register_hook(
        &self,
        global_hook_id: StringID,
    ) -> Result<HookID, Error> {
        let mut hook = Hook::import(&global_hook_id)?;
        use rand::Rng;
        hook.envs.push((String::from("GLOBAL_HOOK_ID"), global_hook_id.clone()));
        let hook_id = loop {
            // Loop to ensure a unique hook ID.
            let id: u32 = rand::thread_rng().gen();
            let hook_id = format!("{}-{}", &global_hook_id, id);
            let mut hooks = self.hooks.lock().unwrap();
            if !hooks.contains_key(&hook_id) {
                hook.envs.push((String::from("HOOK_ID"), hook_id.clone()));
                hooks.insert(hook_id.clone(), hook);
                break hook_id;
            }
        };

        // Create directories for its output tags.
        info!("registered hook {}", &hook_id);
        Ok(hook_id)
    }

    pub fn clear_intervals(&self) {
        // TODO: keep handles for every interval schedule and be able
        // to cancel them if necessary.
    }

    pub fn set_interval(&self, hook_id: HookID, duration: Duration) {
        let tx = self.tx.as_ref().unwrap().clone();
        tokio::spawn(async move {
            let mut interval = time::interval(duration);
            loop {
                interval.tick().await;
                tx.send(QueuedHook{
                    id: hook_id.clone(),
                    trigger: None,
                }).await.unwrap();
            }
        });
    }

    pub fn watch_tag(&self, hook_id: HookID, tag: String) {
        self.watched_tags.write().unwrap()
            .entry(tag)
            .or_insert(vec![])
            .push(hook_id.clone());
    }

    pub async fn spawn_if_watched(
        &self,
        tag: &String,
        timestamp: &String,
        data: &Vec<u8>,
    ) -> usize {
        let hook_ids = {
            let mut hook_ids = HashSet::new();
            let watched_tags = self.watched_tags.read().unwrap();
            if let Some(hooks) = watched_tags.get(tag) {
                for hook_id in hooks {
                    hook_ids.insert(hook_id.clone());
                }
            }
            hook_ids
        };
        let tx = self.tx.as_ref().unwrap();
        let spawned = hook_ids.len();
        // TODO: avoid cloning data unnecessarily.
        for hook_id in hook_ids {
            debug!("spawning {} from {}", hook_id, tag);
            tx.send(QueuedHook {
                id: hook_id,
                trigger: Some((tag.clone(), timestamp.clone(), data.clone())),
            }).await.unwrap();
        }
        spawned
    }

    async fn start_queue_manager(
        mut rx: mpsc::Receiver<QueuedHook>,
        hooks: Arc<Mutex<HashMap<HookID, Hook>>>,
        audit_log: Arc<Mutex<AuditLog>>,
        scheduler: Arc<Mutex<HostScheduler>>,
        mock_send_compute: bool,
    ) {
        loop {
            let next: QueuedHook = rx.recv().await.unwrap();
            let now = Instant::now();

            // Find an available host.
            let hook_id = next.id;
            let process_id = gen_process_id();
            let hosts = loop {
                let hosts = scheduler.lock().unwrap().find_hosts(&hook_id);
                if !hosts.is_empty() {
                    break hosts;
                }
                time::sleep(Duration::from_secs(1)).await;
            };
            let host = &hosts[0];
            let host_addr = format!("http://{}:{}", host.ip, host.port);
            debug!("find a host => {} s", now.elapsed().as_secs_f32());

            // Generate a compute request based on the queued hook.
            let mut request = if let Some(hook) = hooks.lock().unwrap().get(&hook_id) {
                match hook_to_compute_request(
                    &hook,
                    host.host_token.clone(),
                    hook_id.clone(),
                    host.cached,
                ) {
                    Ok(req) => req,
                    Err(e) => {
                        error!("Error converting hook {:?}: {:?}", hook_id, e);
                        continue;
                    },
                }
            } else {
                warn!("queued missing hook");
                continue;
            };
            if let Some((tag, timestamp, data)) = next.trigger {
                request.triggered_tag = tag;
                request.triggered_timestamp = timestamp;
                request.triggered_data = data;
            }
            debug!("convert hook to compute request => {} s", now.elapsed().as_secs_f32());

            // Send the request.
            let process_token = if !mock_send_compute {
                match crate::net::send_compute(&host_addr, request).await {
                    Ok(result) => result.into_inner().process_token,
                    Err(e) => {
                        error!("error spawning hook {}: {:?}", hook_id, e);
                        // TODO: retry?
                        continue;
                    },
                }
            } else {
                warn!("generated process token for testing");
                Token::gen()
            };

            // Update internal data structures.
            // In particular, mark which host is doing the computation.
            // Then log the process start.
            info!("started process_id={} hook_id={} => {} s",
                process_id, hook_id, now.elapsed().as_secs_f32());
            scheduler.lock().unwrap().notify_start(
                host.host_token.clone(),
                hook_id.clone(),
                process_token.clone(),
            );
            // audit_log.lock().unwrap().notify_start(process_token, process_id, hook_id);
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::path::Path;
    use std::net::SocketAddr;
    const PASSWORD: &str = "password";

    fn init_audit_log() -> Arc<Mutex<AuditLog>> {
        let data_path = Path::new("/home/data").to_path_buf();
        Arc::new(Mutex::new(AuditLog::new(data_path)))
    }

    fn init_scheduler(nhosts: usize) -> Arc<Mutex<HostScheduler>> {
        let mut scheduler = HostScheduler::new(PASSWORD);
        for i in 0..nhosts {
            let name = format!("host{}", i);
            let addr: SocketAddr = format!("0.0.0.0:808{}", i).parse().unwrap();
            let token = scheduler.add_host(name, addr, true, PASSWORD).unwrap();
            scheduler.heartbeat(token);
        }
        Arc::new(Mutex::new(scheduler))
    }

    #[tokio::test]
    async fn test_start_creates_a_mpsc_channel() {
        let audit_log = init_audit_log();
        let scheduler = init_scheduler(1);
        let mut runner = HookRunner::new();
        assert!(runner.tx.is_none());
        runner.start(audit_log, scheduler, true);
        assert!(runner.tx.is_some());
    }

    #[tokio::test]
    async fn test_register_hook_adds_a_hook() {
        let audit_log = init_audit_log();
        let scheduler = init_scheduler(0);
        let mut runner = HookRunner::new();
        runner.start(audit_log, scheduler, true);
        assert!(runner.hooks.lock().unwrap().is_empty());
        assert!(HookRunner::register_hook(
            "hello-world".to_string(),
            vec![],
            vec![],
            vec![],
            vec![],
            runner.hooks.clone(),
            runner.tx.unwrap().clone(),
        ).is_ok());
        assert_eq!(runner.hooks.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_register_nonexistent_hook_errors() {
        let audit_log = init_audit_log();
        let scheduler = init_scheduler(0);
        let mut runner = HookRunner::new();
        runner.start(audit_log, scheduler, true);
        assert!(HookRunner::register_hook(
            "goodbye".to_string(),
            vec![],
            vec![],
            vec![],
            vec![],
            runner.hooks.clone(),
            runner.tx.unwrap().clone(),
        ).is_err());
    }

    /*
    #[tokio::test]
    async fn test_register_hook_propagates_fields() {
        let audit_log = init_audit_log();
        let scheduler = init_scheduler(0);
        let mut runner = HookRunner::new();
        runner.start(audit_log, scheduler, true);
        let state_perm = vec!["camera".to_string()];
        let network_perm = vec!["https://www.stanford.edu".to_string()];
        let envs = vec!["KEY=VALUE".to_string()];
        let hook_id = HookRunner::register_hook(
            "hello-world".to_string(),
            state_perm.clone(),
            network_perm.clone(),
            envs.clone(),
            runner.hooks.clone(),
            runner.tx.unwrap().clone(),
        ).unwrap();
        assert!(runner.hooks.lock().unwrap().contains_key(&hook_id),
            "hook ID was properly assigned");
        let hook = runner.hooks.lock().unwrap().get(&hook_id).unwrap().clone();
        assert_eq!(hook.global_hook_id, "hello-world".to_string());
        assert!(hook_id != hook.global_hook_id,
            "hook ID differs from the global hook ID");
        assert_eq!(hook.state_perm, state_perm);
        assert_eq!(hook.network_perm, network_perm);
        assert_eq!(hook.file_perm, file_perm);
        assert!(!hook.package.is_empty());
        assert_eq!(hook.binary_path, Path::new("./main").to_path_buf());
        assert!(hook.args.is_empty());
        assert_eq!(hook.envs, vec![("KEY".to_string(), "VALUE".to_string())]);
    }

    #[tokio::test]
    async fn test_queue_manager_pulls_interval_hooks() {
        let audit_log = init_audit_log();
        let scheduler = init_scheduler(2);
        let mut runner = HookRunner::new();
        runner.start(audit_log, scheduler, true);
        assert_eq!(runner.process_tokens.lock().unwrap().len(), 0,
            "there are no request tokens initially");
        HookRunner::register_hook(
            "hello-world".to_string(), // interval: 5s
            vec![],
            vec![],
            vec![],
            vec![],
            runner.hooks.clone(),
            runner.tx.unwrap().clone(),
        ).unwrap();
        time::sleep(Duration::from_secs(1)).await;
        assert_eq!(runner.process_tokens.lock().unwrap().len(), 1,
            "one request is queued");
        time::sleep(Duration::from_secs(5)).await;
        assert_eq!(runner.process_tokens.lock().unwrap().len(), 2,
            "after 5 seconds (the interval), two requests are queued");
    }

    #[tokio::test]
    async fn test_register_watch_file_hook() {
        let audit_log = init_audit_log();
        let scheduler = init_scheduler(2);
        let mut runner = HookRunner::new(scheduler);
        runner.start(audit_log, scheduler, true);
        HookRunner::register_hook(
            "hello-world-watch".to_string(), // interval: 5s
            vec![],
            vec![],
            vec![],
            vec![],
            runner.hooks.clone(),
            runner.tx.unwrap().clone(),
        ).unwrap();
        time::sleep(Duration::from_secs(1)).await;
        assert_eq!(runner.process_tokens.lock().unwrap().len(), 0,
            "watched file does not queue process automatically");
    }

    #[tokio::test]
    async fn test_queue_manager_pulls_hook_ids_from_queue() {
        let audit_log = init_audit_log();
        let scheduler = init_scheduler(2);
        let mut runner = HookRunner::new(scheduler);
        runner.start(audit_log, scheduler, true);
        let tx = runner.tx.unwrap().clone();
        let hook_id = HookRunner::register_hook(
            "hello-world-watch".to_string(), // interval: 5s
            vec![],
            vec![],
            vec![],
            vec![],
            runner.hooks.clone(),
            tx.clone(),
        ).unwrap();
        HookRunner::queue_hook(hook_id.clone(), tx.clone()).await;
        time::sleep(Duration::from_secs(1)).await;
        assert_eq!(runner.process_tokens.lock().unwrap().len(), 1,
            "queuing existing hook id creates a new request token");
        HookRunner::queue_hook("goodbye".to_string(), tx.clone()).await;
        time::sleep(Duration::from_secs(1)).await;
        assert_eq!(runner.process_tokens.lock().unwrap().len(), 1,
            "queuing nonexistent hook id doesn't do anything");
        HookRunner::queue_hook(hook_id.clone(), tx.clone()).await;
        time::sleep(Duration::from_secs(1)).await;
        assert_eq!(runner.process_tokens.lock().unwrap().len(), 2,
            "queuing existing hook id still works");
        HookRunner::queue_hook(hook_id.clone(), tx.clone()).await;
        time::sleep(Duration::from_secs(1)).await;
        assert_eq!(runner.process_tokens.lock().unwrap().len(), 2,
            "queuing existing hook id ran out of hosts");
    }
    */
}
