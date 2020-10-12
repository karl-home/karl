use astro_dnssd::register::DNSServiceBuilder;
use tokio::runtime::Runtime;

/// Register the Karl service using DNS-SD.
///
/// The service is named "KarlService-<SERVICE_ID>" with type "_karl._tcp".
/// Non-macOS services need to install the appropriate shims around DNS-SD.
/// This has not been tested on other platforms.
///
/// Parameters:
/// - `rt`: The Tokio runtime.
/// - `id`: The Karl service ID.
/// - `port`: The port the service is listening on.
pub fn register(rt: &mut Runtime, id: u32, port: u16) {
    rt.spawn(async move {
        let mut service = DNSServiceBuilder::new("_karl._tcp")
            .with_port(port)
            .with_name(&format!("KarlService-{}", id))
            .build()
            .unwrap();
        let _result = service.register(|reply| match reply {
            Ok(reply) => info!("successful register: {:?}", reply),
            Err(e) => info!("error registering: {:?}", e),
        });
        loop {
            if service.has_data() {
                service.process_result();
            }
        }
    });
}
