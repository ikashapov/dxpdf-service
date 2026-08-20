//! Windows SCM integration: service entry point, install/uninstall helpers.

use std::ffi::OsString;
use std::time::Duration;

use windows_service::service::{
    ServiceAccess, ServiceControl, ServiceControlAccept, ServiceErrorControl, ServiceExitCode,
    ServiceInfo, ServiceStartType, ServiceState, ServiceStatus, ServiceType,
};
use windows_service::service_control_handler::{self, ServiceControlHandlerResult};
use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};
use windows_service::{define_windows_service, service_dispatcher};

use crate::{ServeArgs, SERVICE_DISPLAY_NAME, SERVICE_NAME};

define_windows_service!(ffi_service_main, service_main);

/// Stashes the parsed CLI args for `service_main`, which the SCM invokes with
/// its own (empty) argument list on a separate thread.
static SERVE_ARGS: std::sync::OnceLock<ServeArgs> = std::sync::OnceLock::new();

/// Blocks on the SCM dispatcher until the service is stopped.
pub fn run(args: ServeArgs) -> windows_service::Result<()> {
    let _ = SERVE_ARGS.set(args);
    service_dispatcher::start(SERVICE_NAME, ffi_service_main)
}

fn service_main(_scm_args: Vec<OsString>) {
    if let Err(e) = run_service() {
        log::error!("service failed: {e}");
    }
}

fn run_service() -> windows_service::Result<()> {
    let args = SERVE_ARGS
        .get()
        .expect("serve args set before dispatch")
        .clone();
    let config = args.config();

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let mut shutdown_tx = Some(shutdown_tx);

    let status_handle =
        service_control_handler::register(SERVICE_NAME, move |event| match event {
            ServiceControl::Stop => {
                if let Some(tx) = shutdown_tx.take() {
                    let _ = tx.send(());
                }
                ServiceControlHandlerResult::NoError
            }
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            _ => ServiceControlHandlerResult::NotImplemented,
        })?;

    let running_status = |state: ServiceState| ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: state,
        controls_accepted: ServiceControlAccept::STOP,
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: Duration::default(),
        process_id: None,
    };

    status_handle.set_service_status(running_status(ServiceState::Running))?;
    log::info!("service running: {config:?}");

    let runtime = tokio::runtime::Runtime::new().expect("failed to start tokio runtime");
    let result = runtime.block_on(crate::server::serve(config, async {
        let _ = shutdown_rx.await;
        log::info!("SCM stop received, shutting down");
    }));
    if let Err(e) = &result {
        log::error!("server error: {e}");
    }

    status_handle.set_service_status(ServiceStatus {
        exit_code: match result {
            Ok(()) => ServiceExitCode::Win32(0),
            Err(_) => ServiceExitCode::ServiceSpecific(1),
        },
        ..running_status(ServiceState::Stopped)
    })?;
    Ok(())
}

/// Registers the service (auto-start, LocalSystem) and starts it.
pub fn install(args: &ServeArgs) -> Result<(), Box<dyn std::error::Error>> {
    let manager =
        ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CREATE_SERVICE)?;
    let service_info = ServiceInfo {
        name: OsString::from(SERVICE_NAME),
        display_name: OsString::from(SERVICE_DISPLAY_NAME),
        service_type: ServiceType::OWN_PROCESS,
        start_type: ServiceStartType::AutoStart,
        error_control: ServiceErrorControl::Normal,
        executable_path: std::env::current_exe()?,
        launch_arguments: args.to_service_args(),
        dependencies: vec![],
        account_name: None, // LocalSystem
        account_password: None,
    };
    let service = manager.create_service(
        &service_info,
        ServiceAccess::START | ServiceAccess::QUERY_STATUS,
    )?;
    service.start::<&str>(&[])?;
    Ok(())
}

/// Stops (if running) and deletes the service registration.
pub fn uninstall() -> Result<(), Box<dyn std::error::Error>> {
    let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)?;
    let service = manager.open_service(
        SERVICE_NAME,
        ServiceAccess::STOP | ServiceAccess::DELETE | ServiceAccess::QUERY_STATUS,
    )?;
    let status = service.query_status()?;
    if status.current_state != ServiceState::Stopped {
        let _ = service.stop();
        // Give the server a moment to release the port before deletion.
        for _ in 0..20 {
            if service.query_status()?.current_state == ServiceState::Stopped {
                break;
            }
            std::thread::sleep(Duration::from_millis(250));
        }
    }
    service.delete()?;
    Ok(())
}
