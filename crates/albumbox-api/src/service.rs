//! Windows Service integration: `albumbox-api service install|uninstall|run`.

use std::{ffi::OsString, sync::Mutex, time::Duration};

use anyhow::Context;
use windows_service::{
    define_windows_service,
    service::{
        ServiceAccess, ServiceControl, ServiceControlAccept, ServiceErrorControl, ServiceExitCode, ServiceInfo,
        ServiceStartType, ServiceState, ServiceStatus, ServiceType,
    },
    service_control_handler::{self, ServiceControlHandlerResult},
    service_dispatcher,
    service_manager::{ServiceManager, ServiceManagerAccess},
};

use crate::{config::Config, logging, server};

pub const SERVICE_NAME: &str = "AlbumBoxService";
const DISPLAY_NAME: &str = "AlbumBox";
const DESCRIPTION: &str = "AlbumBox aile fotoğraf ve video arşivi API servisi";

define_windows_service!(ffi_service_main, service_main);

/// Entry point when started by the Service Control Manager.
pub fn run_dispatcher() -> anyhow::Result<()> {
    service_dispatcher::start(SERVICE_NAME, ffi_service_main).context("start service dispatcher")
}

fn service_main(_args: Vec<OsString>) {
    if let Err(e) = run_service() {
        tracing::error!(error = ?e, "service failed");
    }
}

fn run_service() -> anyhow::Result<()> {
    let config = Config::load(None)?;
    let _log_guard = logging::init(&config, false);

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let shutdown_tx = Mutex::new(Some(shutdown_tx));
    let status = service_control_handler::register(SERVICE_NAME, move |event| match event {
        ServiceControl::Stop | ServiceControl::Shutdown => {
            if let Some(tx) = shutdown_tx.lock().unwrap().take() {
                let _ = tx.send(());
            }
            ServiceControlHandlerResult::NoError
        }
        ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
        _ => ServiceControlHandlerResult::NotImplemented,
    })?;

    let set_state = |state: ServiceState, exit: u32| {
        status.set_service_status(ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: state,
            controls_accepted: if state == ServiceState::Running {
                ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN
            } else {
                ServiceControlAccept::empty()
            },
            exit_code: ServiceExitCode::Win32(exit),
            checkpoint: 0,
            wait_hint: Duration::from_secs(10),
            process_id: None,
        })
    };

    set_state(ServiceState::Running, 0)?;
    let result = tokio::runtime::Runtime::new()?.block_on(server::run(config, async {
        let _ = shutdown_rx.await;
    }));
    if let Err(e) = &result {
        tracing::error!(error = ?e, "server error");
    }
    set_state(ServiceState::Stopped, if result.is_ok() { 0 } else { 1 })?;
    result
}

pub fn install() -> anyhow::Result<()> {
    let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE)
        .context("open service manager (yönetici olarak çalıştırın)")?;
    let info = ServiceInfo {
        name: SERVICE_NAME.into(),
        display_name: DISPLAY_NAME.into(),
        service_type: ServiceType::OWN_PROCESS,
        start_type: ServiceStartType::AutoStart,
        error_control: ServiceErrorControl::Normal,
        executable_path: std::env::current_exe()?,
        launch_arguments: vec!["service".into(), "run".into()],
        dependencies: vec![],
        account_name: None, // LocalSystem: can read removable drives
        account_password: None,
    };
    let service = manager.create_service(&info, ServiceAccess::CHANGE_CONFIG | ServiceAccess::START)?;
    service.set_description(DESCRIPTION)?;
    service.start::<OsString>(&[])?;
    println!("{SERVICE_NAME} kuruldu ve başlatıldı.");
    Ok(())
}

pub fn uninstall() -> anyhow::Result<()> {
    let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)
        .context("open service manager (yönetici olarak çalıştırın)")?;
    let service =
        manager.open_service(SERVICE_NAME, ServiceAccess::QUERY_STATUS | ServiceAccess::STOP | ServiceAccess::DELETE)?;
    if service.query_status()?.current_state != ServiceState::Stopped {
        service.stop()?;
        for _ in 0..30 {
            if service.query_status()?.current_state == ServiceState::Stopped {
                break;
            }
            std::thread::sleep(Duration::from_millis(500));
        }
    }
    service.delete()?;
    println!("{SERVICE_NAME} kaldırıldı.");
    Ok(())
}
