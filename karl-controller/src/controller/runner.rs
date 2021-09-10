use std::sync::{Arc, Mutex, RwLock};
use std::collections::{HashSet, HashMap};
use tokio::sync::mpsc;
use std::time::Instant;
use tokio::time::{self, Duration};
use tokio::task::JoinHandle;
use tokio::runtime::Handle;
use karl_common::*;
use crate::controller::HostScheduler;
use crate::protos::ComputeRequest;
use crate::controller::tags::Tags;

#[derive(Debug)]
struct Trigger {
    tag: String,
    timestamp: String,
    data: Vec<u8>,
}

#[derive(Debug)]
struct QueuedModule {
    id: ModuleID,
    trigger: Option<Trigger>,
}

#[derive(Clone)]
pub struct Runner {
    handle: Handle,
    tx: Option<mpsc::Sender<QueuedModule>>,
    /// Watched tags and the modules they spawn.
    watched_tags: Arc<RwLock<HashMap<Tag, Vec<ModuleID>>>>,
    /// Wether to include triggered data in the request.
    pubsub_enabled: bool,
}

#[derive(Default)]
pub struct ModuleConfig {
    // Interval duration and abortable thread handle.
    interval: Option<(u32, JoinHandle<()>)>,
    // Network domains.
    network_perm: HashSet<String>,
    // Environment variables.
    envs: HashMap<String, String>,
}

#[derive(Default)]
pub struct Modules {
    modules: HashMap<ModuleID, Module>,
    tags_inner: HashMap<ModuleID, Tags>,
    config_inner: HashMap<ModuleID, ModuleConfig>,
}

impl ModuleConfig {
    pub fn set_network_perm(&mut self, domains: Vec<String>) {
        self.network_perm = domains.into_iter().collect();
    }

    pub fn get_network_perms(&self) -> &HashSet<String> {
        &self.network_perm
    }

    pub fn get_interval(&self) -> Option<u32> {
        self.interval.as_ref().map(|(duration, _)| *duration)
    }

    fn set_interval(&mut self, duration_s: u32, handle: JoinHandle<()>) {
        self.interval = Some((duration_s, handle));
    }
}

impl Modules {
    pub fn add_module(
        &mut self,
        global_id: &GlobalModuleID,
        id: &ModuleID,
    ) -> Result<ModuleID, Error> {
        let module = Module::import(global_id)?;
        if !self.modules.contains_key(id) {
            self.tags_inner.insert(id.to_string(), Tags::new_module(&module, id.clone()));
            self.modules.insert(id.to_string(), module);
            let mut config = ModuleConfig::default();
            config.envs.insert("GLOBAL_MODULE_ID".to_string(), global_id.to_string());
            config.envs.insert("MODULE_ID".to_string(), id.to_string());
            self.config_inner.insert(id.to_string(), config);
            info!("registered module {} ({})", id, global_id);
            Ok(id.to_string())
        } else {
            error!("module id already exists: {} ({})", id, global_id);
            Err(Error::ModuleInstallError("module id already exists".to_string()))
        }
    }

    /// Removes the module. Checks if all outgoing edges are removed, and
    /// that the network edges and intervals are reset, but does not check
    /// incoming edges or watched tags.
    pub fn remove_module(&mut self, module_id: ModuleID) -> Result<(), Error> {
        if let Some(module) = self.modules.remove(&module_id) {
            let tags = self.tags_inner.remove(&module_id).unwrap();
            let config = self.config_inner.remove(&module_id).unwrap();
            for output in module.returns {
                if !tags.get_output_tags(&output)?.is_empty() {
                    return Err(Error::BadRequest);
                }
            }
            if config.interval.is_some() || !config.network_perm.is_empty() {
                Err(Error::BadRequest)
            } else {
                Ok(())
            }
        } else {
            Err(Error::NotFound)
        }
    }

    pub fn get_module(&self, id: &ModuleID) -> Option<&Module> {
        self.modules.get(id)
    }

    pub fn list_modules(&self) -> Vec<(&ModuleID, &Module)> {
        self.modules.iter().collect()
    }

    pub fn module_exists(&self, id: &ModuleID) -> bool {
        self.modules.contains_key(id)
    }

    pub fn tags(&self, id: &ModuleID) -> Result<&Tags, Error> {
        if let Some(tags) = self.tags_inner.get(id) {
            Ok(tags)
        } else {
            debug!("module {} does not exist", id);
            Err(Error::NotFound)
        }
    }

    pub fn tags_mut(&mut self, id: &ModuleID) -> Result<&mut Tags, Error> {
        if let Some(tags) = self.tags_inner.get_mut(id) {
            Ok(tags)
        } else {
            debug!("module {} does not exist", id);
            Err(Error::NotFound)
        }
    }

    pub fn config(&self, id: &ModuleID) -> Result<&ModuleConfig, Error> {
        if let Some(config) = self.config_inner.get(id) {
            Ok(config)
        } else {
            debug!("module {} does not exist", id);
            Err(Error::NotFound)
        }
    }

    pub fn config_mut(&mut self, id: &ModuleID) -> Result<&mut ModuleConfig, Error> {
        if let Some(config) = self.config_inner.get_mut(id) {
            Ok(config)
        } else {
            debug!("module {} does not exist", id);
            Err(Error::NotFound)
        }
    }

    /// Converts the module to a protobuf compute request.
    ///
    /// The caller must set the request token before sending the compute
    /// reuqest to a host over the network.
    fn get_compute_request(
        &self,
        module_id: String,
        host_token: HostToken,
        cached: bool,
    ) -> Result<ComputeRequest, Error> {
        let module = self.get_module(&module_id).ok_or(
            Error::NotFoundInfo(format!("module not found: {}", module_id)))?;
        let config = self.config(&module_id)?;
        let package = if cached {
            vec![]
        } else {
            module.package.clone()
        };
        let binary_path = module.binary_path.clone().into_os_string().into_string().unwrap();
        let args = module.args.clone().into_iter().collect();
        let envs = config.envs.clone().iter().map(|(k, v)| format!("{}={}", k, v)).collect();
        let network_perm = config.network_perm.clone().into_iter().collect();
        Ok(ComputeRequest {
            host_token,
            cached,
            package,
            binary_path,
            args,
            envs,
            params: self.tags(&module_id)?.inputs_string(),
            returns: self.tags(&module_id)?.outputs_string(),
            network_perm,
            triggered_tag: String::new(),
            triggered_timestamp: String::new(),
            triggered_data: Vec::new(),
            module_id,
        })
    }
}

fn gen_process_id() -> ProcessID {
    use rand::Rng;
    rand::thread_rng().gen()
}

impl Runner {
    /// Create a new Runner.
    pub fn new(
        handle: Handle,
        pubsub_enabled: bool,
        watched_tags: Arc<RwLock<HashMap<Tag, Vec<ModuleID>>>>,
    ) -> Self {
        Self {
            handle,
            tx: None,
            watched_tags,
            pubsub_enabled,
        }
    }

    /// Spawns queue manager to monitor the delay queue for when modules
    /// are read to be scheduled. Add modules to the queue via `queue_module()`.
    pub fn start(
        &mut self,
        modules: Arc<RwLock<Modules>>,
        scheduler: Arc<Mutex<HostScheduler>>,
        mock_send_compute: bool,
    ) {
        let buffer = 100;  // TODO: tune
        let (tx, rx) = mpsc::channel::<QueuedModule>(buffer);
        self.tx = Some(tx);
        self.handle.spawn(async move {
            Self::start_queue_manager(
                rx,
                modules,
                scheduler,
                mock_send_compute,
            ).await;
        });
    }

    fn remove_interval(
        &self,
        module_id: &ModuleID,
        modules: &mut Modules,
    ) -> Result<(), Error> {
        if let Some((_, abort_handle)) = &modules.config(module_id)?.interval {
            abort_handle.abort();
        } else {
            error!("module {} does not have an interval", module_id);
            return Err(Error::NotFound);
        }
        modules.config_mut(module_id)?.interval = None;
        Ok(())
    }

    pub fn set_interval(
        &self,
        module_id: ModuleID,
        seconds: Option<u32>,
        modules: &mut Modules,
    ) -> Result<(), Error> {
        let tx = self.tx.as_ref().unwrap().clone();
        if modules.config(&module_id)?.interval.is_some() {
            self.remove_interval(&module_id, modules)?;
        }
        if let Some(seconds) = seconds {
            let duration = Duration::from_secs(seconds.into());
            let config = modules.config_mut(&module_id)?;
            let handle = self.handle.spawn(async move {
                let mut interval = time::interval(duration);
                loop {
                    interval.tick().await;
                    warn!("start true_pipeline: {:?}", Instant::now());
                    tx.send(QueuedModule{
                        id: module_id.clone(),
                        trigger: None,
                    }).await.unwrap();
                }
            });
            config.set_interval(seconds, handle);
        }
        Ok(())
    }

    pub fn watch_tag(&self, module_id: ModuleID, tag: String) {
        self.watched_tags.write().unwrap()
            .entry(tag)
            .or_insert(vec![])
            .push(module_id.clone());
    }

    pub fn unwatch_tag(&self, module_id: ModuleID, tag: &String) -> Result<(), Error> {
        let mut watched_tags = self.watched_tags.write().unwrap();
        if let Some(module_ids) = watched_tags.get_mut(tag) {
            if let Some(index) = module_ids.iter().position(|id| id == &module_id) {
                module_ids.remove(index);
                Ok(())
            } else {
                Err(Error::NotFound)
            }
        } else {
            Err(Error::NotFound)
        }
    }

    pub async fn spawn_module_if_watched(
        &self,
        tag: &String,
        timestamp: &String,
        data: &Vec<u8>,
    ) -> usize {
        let module_ids = self.watched_tags.read().unwrap()
            .get(tag).unwrap_or(&vec![]).clone();
        let tx = self.tx.as_ref().unwrap();
        let spawned = module_ids.len();
        // TODO: avoid cloning data unnecessarily.
        for module_id in module_ids {
            debug!("spawning {} from {}", module_id, tag);
            let data = if self.pubsub_enabled {
                data.clone()
            } else {
                vec![]
            };
            tx.send(QueuedModule {
                id: module_id,
                trigger: Some(Trigger {
                    tag: tag.clone(),
                    timestamp: timestamp.clone(),
                    data,
                }),
            }).await.unwrap();
        }
        spawned
    }

    pub async fn spawn_module(&self, module_id: String) {
        self.tx.as_ref().unwrap().send(QueuedModule {
            id: module_id,
            trigger: None,
        }).await.unwrap();
    }

    async fn start_queue_manager(
        mut rx: mpsc::Receiver<QueuedModule>,
        modules: Arc<RwLock<Modules>>,
        scheduler: Arc<Mutex<HostScheduler>>,
        mock_send_compute: bool,
    ) {
        loop {
            let next: QueuedModule = rx.recv().await.unwrap();
            let now = Instant::now();

            // Find an available host.
            let module_id = next.id;
            let process_id = gen_process_id();
            let hosts = loop {
                let hosts = scheduler.lock().unwrap().find_hosts(&module_id);
                if !hosts.is_empty() {
                    break hosts;
                }
                time::sleep(Duration::from_secs(1)).await;
            };
            let host = &hosts[0];
            let host_addr = format!("http://{}:{}", host.ip, host.port);
            debug!("find a host => {} s", now.elapsed().as_secs_f32());

            // Generate a compute request based on the queued module.
            let mut request = {
                match modules.read().unwrap().get_compute_request(
                    module_id.clone(),
                    host.host_token.clone(),
                    host.cached,
                ) {
                    Ok(req) => req,
                    Err(e) => {
                        error!("Error converting module {:?}: {:?}", module_id, e);
                        continue;
                    },
                }
            };
            if let Some(trigger) = next.trigger {
                request.triggered_tag = trigger.tag;
                request.triggered_timestamp = trigger.timestamp;
                request.triggered_data = trigger.data;
            }
            debug!("convert module to compute request => {} s", now.elapsed().as_secs_f32());

            // Send the request.
            let process_token = if !mock_send_compute {
                match crate::net::send_compute(&host_addr, request).await {
                    Ok(result) => result.into_inner().process_token,
                    Err(e) => {
                        error!("error spawning module {}: {:?}", module_id, e);
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
            info!("started process_id={} module_id={} => {} s",
                process_id, module_id, now.elapsed().as_secs_f32());
            scheduler.lock().unwrap().notify_start(
                host.host_token.clone(),
                module_id.clone(),
                process_token.clone(),
            );
        }
    }
}

/*
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
        let mut runner = Runner::new();
        assert!(runner.tx.is_none());
        runner.start(audit_log, scheduler, true);
        assert!(runner.tx.is_some());
    }

    #[tokio::test]
    async fn test_register_module_adds_a_module() {
        let audit_log = init_audit_log();
        let scheduler = init_scheduler(0);
        let mut runner = Runner::new();
        runner.start(audit_log, scheduler, true);
        assert!(runner.modules.lock().unwrap().is_empty());
        assert!(Runner::register_module(
            "hello-world".to_string(),
            vec![],
            vec![],
            vec![],
            vec![],
            runner.modules.clone(),
            runner.tx.unwrap().clone(),
        ).is_ok());
        assert_eq!(runner.modules.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_register_nonexistent_module_errors() {
        let audit_log = init_audit_log();
        let scheduler = init_scheduler(0);
        let mut runner = Runner::new();
        runner.start(audit_log, scheduler, true);
        assert!(Runner::register_module(
            "goodbye".to_string(),
            vec![],
            vec![],
            vec![],
            vec![],
            runner.modules.clone(),
            runner.tx.unwrap().clone(),
        ).is_err());
    }

    #[tokio::test]
    async fn test_register_module_propagates_fields() {
        let audit_log = init_audit_log();
        let scheduler = init_scheduler(0);
        let mut runner = Runner::new();
        runner.start(audit_log, scheduler, true);
        let state_perm = vec!["camera".to_string()];
        let network_perm = vec!["https://www.stanford.edu".to_string()];
        let envs = vec!["KEY=VALUE".to_string()];
        let module_id = Runner::register_module(
            "hello-world".to_string(),
            state_perm.clone(),
            network_perm.clone(),
            envs.clone(),
            runner.modules.clone(),
            runner.tx.unwrap().clone(),
        ).unwrap();
        assert!(runner.modules.lock().unwrap().contains_key(&module_id),
            "module ID was properly assigned");
        let module = runner.modules.lock().unwrap().get(&module_id).unwrap().clone();
        assert_eq!(module.global_module_id, "hello-world".to_string());
        assert!(module_id != module.global_module_id,
            "module ID differs from the global module ID");
        assert_eq!(module.state_perm, state_perm);
        assert_eq!(module.network_perm, network_perm);
        assert_eq!(module.file_perm, file_perm);
        assert!(!module.package.is_empty());
        assert_eq!(module.binary_path, Path::new("./main").to_path_buf());
        assert!(module.args.is_empty());
        assert_eq!(module.envs, vec![("KEY".to_string(), "VALUE".to_string())]);
    }

    #[tokio::test]
    async fn test_queue_manager_pulls_interval_modules() {
        let audit_log = init_audit_log();
        let scheduler = init_scheduler(2);
        let mut runner = ModuleRunner::new();
        runner.start(audit_log, scheduler, true);
        assert_eq!(runner.process_tokens.lock().unwrap().len(), 0,
            "there are no request tokens initially");
        ModuleRunner::register_module(
            "hello-world".to_string(), // interval: 5s
            vec![],
            vec![],
            vec![],
            vec![],
            runner.modules.clone(),
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
    async fn test_register_watch_file_module() {
        let audit_log = init_audit_log();
        let scheduler = init_scheduler(2);
        let mut runner = ModuleRunner::new(scheduler);
        runner.start(audit_log, scheduler, true);
        ModuleRunner::register_module(
            "hello-world-watch".to_string(), // interval: 5s
            vec![],
            vec![],
            vec![],
            vec![],
            runner.modules.clone(),
            runner.tx.unwrap().clone(),
        ).unwrap();
        time::sleep(Duration::from_secs(1)).await;
        assert_eq!(runner.process_tokens.lock().unwrap().len(), 0,
            "watched file does not queue process automatically");
    }

    #[tokio::test]
    async fn test_queue_manager_pulls_module_ids_from_queue() {
        let audit_log = init_audit_log();
        let scheduler = init_scheduler(2);
        let mut runner = ModuleRunner::new(scheduler);
        runner.start(audit_log, scheduler, true);
        let tx = runner.tx.unwrap().clone();
        let module_id = ModuleRunner::register_module(
            "hello-world-watch".to_string(), // interval: 5s
            vec![],
            vec![],
            vec![],
            vec![],
            runner.modules.clone(),
            tx.clone(),
        ).unwrap();
        ModuleRunner::queue_module(module_id.clone(), tx.clone()).await;
        time::sleep(Duration::from_secs(1)).await;
        assert_eq!(runner.process_tokens.lock().unwrap().len(), 1,
            "queuing existing module id creates a new request token");
        ModuleRunner::queue_module("goodbye".to_string(), tx.clone()).await;
        time::sleep(Duration::from_secs(1)).await;
        assert_eq!(runner.process_tokens.lock().unwrap().len(), 1,
            "queuing nonexistent module id doesn't do anything");
        ModuleRunner::queue_module(module_id.clone(), tx.clone()).await;
        time::sleep(Duration::from_secs(1)).await;
        assert_eq!(runner.process_tokens.lock().unwrap().len(), 2,
            "queuing existing module id still works");
        ModuleRunner::queue_module(module_id.clone(), tx.clone()).await;
        time::sleep(Duration::from_secs(1)).await;
        assert_eq!(runner.process_tokens.lock().unwrap().len(), 2,
            "queuing existing module id ran out of hosts");
    }

    #[test]
    fn test_to_compute_request_works() {
        // let package = vec![0, 1, 2, 3];
        // let binary_path = "binary_path";
        // let args = vec!["arg1".to_string(), "arg2".to_string()];
        // let envs = vec![("KEY".to_string(), "VALUE".to_string())];
        // let state_perm = vec!["camera".to_string()];
        // let network_perm = vec!["https://www.stanford.edu".to_string()];

        // let module = Module::new(
        //     "module_id".to_string(),
        //     ModuleSchedule::Interval(Duration::from_secs(10)),
        //     state_perm.clone(),
        //     network_perm.clone(),
        //     package.clone(),
        //     binary_path,
        //     args.clone(),
        //     envs.clone(),
        // );
        // let r = module.to_compute_request().unwrap();
        // assert_eq!(r.package, package);
        // assert_eq!(r.binary_path, binary_path);
        // assert_eq!(r.args, args);
        // let expected_envs: Vec<_> =
        //     envs.iter().map(|(k, v)| format!("{}={}", k, v)).collect();
        // assert_eq!(r.envs, expected_envs);
        // assert_eq!(r.state_perm, state_perm);
        // assert_eq!(r.network_perm, network_perm);
        // assert_eq!(r.file_perm.len(), 1);
    }
}
*/