use crate::handler::JulieServerHandler;
use crate::paths::RegistryPaths;
use crate::workspace_runtime::manager::{
    DEFAULT_IDLE_EXPIRY_DURATION, DEFAULT_MAX_IDLE_RUNTIMES, DEFAULT_PROBE_INTERVAL,
    ManagerTestBarriers, WorkspaceRuntimeManager,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

pub struct WorkspaceRuntimeManagerBuilder {
    registry_paths: RegistryPaths,
    template_handler: Option<Arc<JulieServerHandler>>,
    probe_interval: Duration,
    idle_timeout: Duration,
    max_idle_runtimes: usize,
    test_barriers: Option<ManagerTestBarriers>,
    fault_flag: Option<String>,
}

impl WorkspaceRuntimeManagerBuilder {
    pub fn new(paths: RegistryPaths) -> Self {
        Self {
            registry_paths: paths,
            template_handler: None,
            probe_interval: DEFAULT_PROBE_INTERVAL,
            idle_timeout: DEFAULT_IDLE_EXPIRY_DURATION,
            max_idle_runtimes: DEFAULT_MAX_IDLE_RUNTIMES,
            test_barriers: None,
            fault_flag: None,
        }
    }

    pub fn template(&mut self, handler: Arc<JulieServerHandler>) -> &mut Self {
        self.template_handler = Some(handler);
        self
    }

    pub fn probe_interval(&mut self, interval: Duration) -> &mut Self {
        self.probe_interval = interval;
        self
    }

    pub fn idle_timeout(&mut self, timeout: Duration) -> &mut Self {
        self.idle_timeout = timeout;
        self
    }

    pub fn max_idle_runtimes(&mut self, max: usize) -> &mut Self {
        self.max_idle_runtimes = max;
        self
    }

    pub fn test_barriers(&mut self, barriers: ManagerTestBarriers) -> &mut Self {
        self.test_barriers = Some(barriers);
        self
    }

    pub fn inject_fault(&mut self, fault: &str) -> &mut Self {
        self.fault_flag = Some(fault.to_string());
        self
    }

    pub fn build(&self) -> Arc<WorkspaceRuntimeManager> {
        let manager = Arc::new(WorkspaceRuntimeManager {
            registry_paths: self.registry_paths.clone(),
            slots: Arc::new(RwLock::new(HashMap::new())),
            template_handler: self.template_handler.clone(),
            probe_interval: self.probe_interval,
            idle_timeout: self.idle_timeout,
            max_idle_runtimes: self.max_idle_runtimes,
            test_barriers: self.test_barriers.clone(),
            fault_flag: self.fault_flag.clone(),
            eviction_task: std::sync::Mutex::new(None),
        });

        manager.start_eviction_loop();
        manager
    }
}
