use std::sync::{Arc, Mutex, RwLock, atomic::AtomicUsize};
use std::collections::{HashSet, HashMap};
use tokio::sync::mpsc;
use std::time::Instant;
use tonic::{Status, Code};
use tokio::time::{self, Duration};
use karl_common::*;
use crate::controller::HostScheduler;
use crate::protos::ComputeRequest;
use crate::controller::tags::Tags;
use futures::future::{Abortable, AbortHandle};

type Tag = String;
type GlobalModuleID = String;
type ModuleID = String;

#[derive(Default, Clone)]
pub struct ModuleConfig {
    // Interval duration and abortable thread handle.
    interval: Option<(u32, AbortHandle)>,
    // Network domains.
    network_perm: HashSet<String>,
    // Environment variables.
    envs: HashMap<String, String>,
}

#[derive(Default, Clone)]
pub struct Modules {
    tag_counter: Arc<Mutex<AtomicUsize>>,
    modules: HashMap<ModuleID, Module>,
    tags_inner: HashMap<ModuleID, Tags>,
    config_inner: HashMap<ModuleID, ModuleConfig>,
}

impl ModuleConfig {
    pub fn set_interval(&mut self, duration_s: u32, handle: AbortHandle) {
        self.interval = Some((duration_s, handle));
    }

    pub fn add_network_perm(&mut self, domain: &str) {
        self.network_perm.insert(domain.to_string());
    }

    pub fn set_env(&mut self, key: String, value: String) {
        self.envs.insert(key, value);
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
            self.tags_inner.insert(id.to_string(), Tags::new_module(&module));
            self.modules.insert(id.to_string(), module);
            let mut config = ModuleConfig::default();
            config.envs.insert("GLOBAL_MODULE_ID".to_string(), global_id.to_string());
            config.envs.insert("MODULE_ID".to_string(), id.to_string());
            self.config_inner.insert(id.to_string(), config);
            info!("registered module {} ({})", id, global_id);
            Ok(id.to_string())
        } else {
            error!("module id already exists: {} ({})", id, global_id);
            Err(Error::HookInstallError("module id already exists".to_string()))
        }
    }

    pub fn next_tag(&self) -> String {
        let mut tag_counter = self.tag_counter.lock().unwrap();
        let tag = tag_counter.get_mut();
        let old_tag = *tag;
        *tag = old_tag + 1;
        format!("t{}", old_tag)
    }

    pub fn get_module(&self, id: &ModuleID) -> Option<&Module> {
        self.modules.get(id)
    }

    pub fn list_modules(&self) -> Vec<&Module> {
        self.modules.values().collect()
    }

    pub fn module_exists(&self, id: &ModuleID) -> bool {
        self.modules.contains_key(id)
    }

    pub fn tags(&self, id: &ModuleID) -> Result<&Tags, Status> {
        if let Some(tags) = self.tags_inner.get(id) {
            Ok(tags)
        } else {
            Err(Status::new(Code::NotFound, format!(
                "module {} does not exist", id)))
        }
    }

    pub fn tags_mut(&mut self, id: &ModuleID) -> Result<&mut Tags, Status> {
        if let Some(tags) = self.tags_inner.get_mut(id) {
            Ok(tags)
        } else {
            Err(Status::new(Code::NotFound, format!(
                "module {} does not exist", id)))
        }
    }

    pub fn config(&self, id: &ModuleID) -> Result<&ModuleConfig, Status> {
        if let Some(config) = self.config_inner.get(id) {
            Ok(config)
        } else {
            Err(Status::new(Code::NotFound, format!(
                "module {} does not exist", id)))
        }
    }

    pub fn config_mut(&mut self, id: &ModuleID) -> Result<&mut ModuleConfig, Status> {
        if let Some(config) = self.config_inner.get_mut(id) {
            Ok(config)
        } else {
            Err(Status::new(Code::NotFound, format!(
                "module {} does not exist", id)))
        }
    }

    // @params map from `module_id` to global_module_id` of desired modules
    // @returns module ids of modules_to_remove and modules_to_add
    pub fn delta(
        &self,
        new_modules: HashMap<ModuleID, GlobalModuleID>,
    ) -> (Vec<ModuleID>, Vec<ModuleID>) {
        let modules_to_remove: Vec<_> = self.modules.keys()
            .filter(|&m| !new_modules.contains_key(m))
            .map(|m| m.to_string())
            .collect();
        let modules_to_add: Vec<_> = new_modules.keys()
            .filter(|&m| !self.modules.contains_key(m))
            .map(|m| m.to_string())
            .collect();
        (modules_to_remove, modules_to_add)
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
    ) -> Result<ComputeRequest, Status> {
        let module = self.get_module(&module_id).unwrap();
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

#[derive(Debug)]
pub struct QueuedModule {
    id: ModuleID,
    /// Tag, timestamp, data
    trigger: Option<(String, String, Vec<u8>)>,
}

#[derive(Clone)]
pub struct Runner {
    tx: Option<mpsc::Sender<QueuedModule>>,
    /// Registered modules and their local module IDs.
    modules: Arc<Mutex<Modules>>,
    /// Watched tags and the modules they spawn.
    watched_tags: Arc<RwLock<HashMap<Tag, Vec<ModuleID>>>>,
    /// Wether to include triggered data in the request.
    pubsub_enabled: bool,
}

fn gen_process_id() -> ProcessID {
    use rand::Rng;
    rand::thread_rng().gen()
}

impl Runner {
    /// Create a new Runner.
    pub fn new(pubsub_enabled: bool, modules: Arc<Mutex<Modules>>) -> Self {
        Self {
            tx: None,
            modules,
            watched_tags: Arc::new(RwLock::new(HashMap::new())),
            pubsub_enabled,
        }
    }

    /// Spawns queue manager to monitor the delay queue for when modules
    /// are read to be scheduled. Add modules to the queue via `queue_module()`.
    pub fn start(
        &mut self,
        scheduler: Arc<Mutex<HostScheduler>>,
        mock_send_compute: bool,
    ) {
        let buffer = 100;  // TODO: tune
        let (tx, rx) = mpsc::channel::<QueuedModule>(buffer);
        self.tx = Some(tx);
        let modules = self.modules.clone();
        tokio::spawn(async move {
            Self::start_queue_manager(
                rx,
                modules,
                scheduler,
                mock_send_compute,
            ).await;
        });
    }

    // fn remove_module(&self, module_id: StringID) -> Result<(), Error> {
    //     let mut modules = self.modules.lock().unwrap();
    //     if let Some(_) = modules.remove(&module_id) {
    //         let mut tags = 0;
    //         for module_ids in self.watched_tags.write().unwrap().values_mut() {
    //             let indexes = module_ids
    //                 .iter()
    //                 .enumerate()
    //                 .filter(|(_, id)| *id == &module_id)
    //                 .map(|(index, _)| index)
    //                 .collect::<Vec<_>>();
    //             for index in indexes {
    //                 module_ids.remove(index);
    //                 tags += 1;
    //             }
    //         }
    //         info!("removed module {} with {} tags", &module_id, tags);
    //         Ok(())
    //     } else {
    //         error!("cannot remove module, does not exist: {}", module_id);
    //         Err(Error::NotFound)
    //     }
    // }

    // pub fn clear_intervals(&self) {
    //     // TODO: keep handles for every interval schedule and be able
    //     // to cancel them if necessary.
    // }

    pub fn set_interval(
        &self,
        module_id: ModuleID,
        seconds: u32,
    ) -> Result<(), Status> {
        let tx = self.tx.as_ref().unwrap().clone();
        let mut modules = self.modules.lock().unwrap();
        if let Some((duration, _)) = modules.config(&module_id)?.interval {
            error!("module {} already has an interval set: {}", module_id, duration);
            Err(Status::new(Code::InvalidArgument, "interval already set"))
        } else {
            let duration = Duration::from_secs(seconds.into());
            let config = modules.config_mut(&module_id)?;
            let (abort_handle, abort_registration) = AbortHandle::new_pair();
            let future = Abortable::new(async move {
                let mut interval = time::interval(duration);
                loop {
                    interval.tick().await;
                    warn!("start true_pipeline: {:?}", Instant::now());
                    tx.send(QueuedModule{
                        id: module_id.clone(),
                        trigger: None,
                    }).await.unwrap();
                }
            }, abort_registration);
            tokio::spawn(async move { future.await });
            config.set_interval(seconds, abort_handle);
            Ok(())
        }
    }

    pub fn watch_tag(&self, module_id: ModuleID, tag: String) {
        self.watched_tags.write().unwrap()
            .entry(tag)
            .or_insert(vec![])
            .push(module_id.clone());
    }

    pub async fn spawn_if_watched(
        &self,
        tag: &String,
        timestamp: &String,
        data: &Vec<u8>,
    ) -> usize {
        let module_ids = {
            let mut module_ids = HashSet::new();
            let watched_tags = self.watched_tags.read().unwrap();
            if let Some(modules) = watched_tags.get(tag) {
                for module_id in modules {
                    module_ids.insert(module_id.clone());
                }
            }
            module_ids
        };
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
                trigger: Some((tag.clone(), timestamp.clone(), data)),
            }).await.unwrap();
        }
        spawned
    }

    pub async fn spawn_module_id(&self, module_id: String) {
        self.tx.as_ref().unwrap().send(QueuedModule {
            id: module_id,
            trigger: None,
        }).await.unwrap();
    }

    async fn start_queue_manager(
        mut rx: mpsc::Receiver<QueuedModule>,
        modules: Arc<Mutex<Modules>>,
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
                match modules.lock().unwrap().get_compute_request(
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
            if let Some((tag, timestamp, data)) = next.trigger {
                request.triggered_tag = tag;
                request.triggered_timestamp = timestamp;
                request.triggered_data = data;
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
}
*/