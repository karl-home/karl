use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use tokio::sync::mpsc;
use tokio::time::{self, Duration};
use crate::controller::HostScheduler;
use crate::controller::types::*;
use crate::hook::{Hook, FileACL, DomainName, HookSchedule};
use crate::common::*;


pub struct HookRunner {
    pub tx: Option<mpsc::Sender<HookID>>,
    /// Registered hooks and their local hook IDs.
    pub(crate) hooks: Arc<Mutex<HashMap<HookID, Hook>>>,
    /// Process tokens for each spawned process. Cleared on NotifyEnd.
    process_tokens: Arc<Mutex<HashMap<ProcessToken, (ProcessID, HookID)>>>,
    /// Scheduler for finding hosts
    scheduler: Arc<Mutex<HostScheduler>>,
}

fn gen_process_id() -> ProcessID {
    use rand::Rng;
    rand::thread_rng().gen()
}

impl HookRunner {
    /// Create a new HookRunner.
    pub fn new(scheduler: Arc<Mutex<HostScheduler>>) -> Self {
        Self {
            tx: None,
            hooks: Arc::new(Mutex::new(HashMap::new())),
            process_tokens: Arc::new(Mutex::new(HashMap::new())),
            scheduler,
        }
    }

    /// Spawns queue manager to monitor the delay queue for when hooks
    /// are read to be scheduled. Add hooks to the queue via `queue_hook()`.
    pub fn start(&mut self, mock_send_compute: bool) {
        let buffer = 100;  // TODO: tune
        let (tx, rx) = mpsc::channel::<HookID>(buffer);
        self.tx = Some(tx);
        let hooks = self.hooks.clone();
        let process_tokens = self.process_tokens.clone();
        let scheduler = self.scheduler.clone();
        tokio::spawn(async move {
            Self::start_queue_manager(
                rx,
                hooks,
                process_tokens,
                scheduler,
                mock_send_compute,
            ).await;
        });
    }

    /// Register a hook.
    ///
    /// Parameters:
    /// - global_hook_id - The ID of the hook from the global hook repository.
    /// - network_perm - Requested network permissions.
    /// - file_perm - Requested file permissions.
    /// - envs - Requested environment variables / configuration `<KEY>=<VALUE>`.
    ///   Replaces the imported environment variables only if non-empty.
    ///
    /// IoError if error importing hook from filesystem.
    /// HookInstallError if environment variables are formatted incorrectly.
    pub fn register_hook(
        global_hook_id: StringID,
        network_perm: Vec<DomainName>,
        file_perm: Vec<FileACL>,
        envs: Vec<String>,
        hooks: Arc<Mutex<HashMap<HookID, Hook>>>,
        tx: mpsc::Sender<HookID>,
    ) -> Result<HookID, Error> {
        let mut hook = Hook::import(&global_hook_id)?
            .set_network_perm(network_perm)
            .set_file_perm(file_perm);
        if !envs.is_empty() {
            hook = hook.set_envs(envs)?;
        }
        let schedule = hook.schedule.clone();
        use rand::Rng;
        let hook_id = loop {
            // Loop to ensure a unique hook ID.
            let id: u32 = rand::thread_rng().gen();
            let hook_id = format!("{}-{}", global_hook_id, id);
            let mut hooks = hooks.lock().unwrap();
            if !hooks.contains_key(&hook_id) {
                hooks.insert(hook_id.clone(), hook);
                break hook_id;
            }
        };

        // Start the hook if on an interval schedule.
        match schedule {
            HookSchedule::Interval(duration) => {
                let hook_id = hook_id.clone();
                tokio::spawn(async move {
                    let mut interval = time::interval(duration);
                    loop {
                        interval.tick().await;
                        Self::queue_hook(hook_id.clone(), tx.clone()).await;
                    }
                });
            },
            HookSchedule::WatchFile(_) => {},
        }
        Ok(hook_id)
    }

    /// Queue a hook to be executed at a specific instant.
    pub async fn queue_hook(hook_id: HookID, tx: mpsc::Sender<HookID>) {
        tx.send(hook_id).await.unwrap();
    }

    async fn start_queue_manager(
        mut rx: mpsc::Receiver<HookID>,
        hooks: Arc<Mutex<HashMap<HookID, Hook>>>,
        process_tokens: Arc<Mutex<HashMap<RequestToken, (ProcessID, HookID)>>>,
        scheduler: Arc<Mutex<HostScheduler>>,
        mock_send_compute: bool,
    ) {
        loop {
            // Generate a compute request based on the queued hook.
            let hook_id: HookID = rx.recv().await.unwrap();
            let request = if let Some(hook) = hooks.lock().unwrap().get(&hook_id) {
                match hook.to_compute_request() {
                    Ok(req) => req,
                    Err(e) => {
                        error!("Error converting hook {:?}: {:?}", hook_id, e);
                        continue;
                    },
                }
            } else {
                continue;
            };

            // Find an available host and prepare the request.
            let host = loop {
                if let Some(host) = scheduler.lock().unwrap().find_host() {
                    break host;
                }
                time::sleep(Duration::from_secs(1)).await;
            };
            let host_addr = format!("{}:{}", host.ip, host.port);

            // Send the request.
            // TODO: check if the result has no error!
            if !mock_send_compute {
                crate::net::send_compute(&host_addr, request).await.unwrap();
            }

            // Update internal data structures.
            // In particular, the process's request token for authentication.
            let process_id = gen_process_id();
            let process_token = process_id.to_string(); // TODO?
            process_tokens.lock().unwrap().insert(
                process_token,
                (process_id, hook_id),
            );
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::path::Path;
    use std::net::SocketAddr;
    const PASSWORD: &str = "password";

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
        let scheduler = init_scheduler(1);
        let mut runner = HookRunner::new(scheduler);
        assert!(runner.tx.is_none());
        runner.start(true);
        assert!(runner.tx.is_some());
    }

    #[tokio::test]
    async fn test_register_hook_adds_a_hook() {
        let scheduler = init_scheduler(0);
        let mut runner = HookRunner::new(scheduler);
        runner.start(true);
        assert!(runner.hooks.lock().unwrap().is_empty());
        assert!(HookRunner::register_hook(
            "hello-world".to_string(),
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
        let scheduler = init_scheduler(0);
        let mut runner = HookRunner::new(scheduler);
        runner.start(true);
        assert!(HookRunner::register_hook(
            "goodbye".to_string(),
            vec![],
            vec![],
            vec![],
            runner.hooks.clone(),
            runner.tx.unwrap().clone(),
        ).is_err());
    }

    #[tokio::test]
    async fn test_register_hook_propagates_fields() {
        let scheduler = init_scheduler(0);
        let mut runner = HookRunner::new(scheduler);
        runner.start(true);
        let network_perm = vec!["https://www.stanford.edu".to_string()];
        let file_perm = vec![FileACL::new("main", true, true)];
        let envs = vec!["KEY=VALUE".to_string()];
        let hook_id = HookRunner::register_hook(
            "hello-world".to_string(),
            network_perm.clone(),
            file_perm.clone(),
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
        assert_eq!(hook.network_perm, network_perm);
        assert_eq!(hook.file_perm, file_perm);
        assert!(!hook.package.is_empty());
        assert_eq!(hook.binary_path, Path::new("./main").to_path_buf());
        assert!(hook.args.is_empty());
        assert_eq!(hook.envs, vec![("KEY".to_string(), "VALUE".to_string())]);
    }

    #[tokio::test]
    async fn test_queue_manager_pulls_interval_hooks() {
        let scheduler = init_scheduler(2);
        let mut runner = HookRunner::new(scheduler);
        runner.start(true);
        assert_eq!(runner.process_tokens.lock().unwrap().len(), 0,
            "there are no request tokens initially");
        HookRunner::register_hook(
            "hello-world".to_string(), // interval: 5s
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
        let scheduler = init_scheduler(2);
        let mut runner = HookRunner::new(scheduler);
        runner.start(true);
        HookRunner::register_hook(
            "hello-world-watch".to_string(), // interval: 5s
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
        let scheduler = init_scheduler(2);
        let mut runner = HookRunner::new(scheduler);
        runner.start(true);
        let tx = runner.tx.unwrap().clone();
        let hook_id = HookRunner::register_hook(
            "hello-world-watch".to_string(), // interval: 5s
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
}
