//! The client for interacting with the Karl controller and services.
//!
//! Each language SDK must re-implement its own client, called an executor.
//! The interactions with other Karl entities are entirely through TCP.
//! The interface must follow the request and result interface defined in
//! `common.rs`.
//!
//! If the request to a host is unsuccessful, the client must query the
//! executor for a different host and try again on the client-side.
//! Addresses are passed in the form of `<IP>:<PORT>`.
use std::net::TcpStream;
use bincode;
use karl_common::Error;
use crate::common::*;
use karl_common::*;

/// Returns a host address given by the controller.
pub fn get_host(controller_addr: &str) -> String {
    // TODO: handle network errors with connecting, writing, reading.
    // Deserialization may also fail due to the other side.
    let mut stream = TcpStream::connect(controller_addr).unwrap();
    let req = HostRequest::default();
    let req_bytes = bincode::serialize(&req)
        .map_err(|e| Error::SerializationError(format!("{:?}", e)))
        .unwrap();
    write_packet(&mut stream, HT_HOST_REQUEST, &req_bytes).unwrap();
    let (header, res_bytes) = &read_packets(&mut stream, 1).unwrap()[0];
    assert_eq!(header.ty, HT_HOST_RESULT);
    let res: HostResult = bincode::deserialize(&res_bytes)
        .map_err(|e| Error::SerializationError(format!("{:?}", e)))
        .unwrap();
    format!("{}:{}", res.ip, res.port)
}

/// Pings the given host and returns the result.
pub fn send_ping(host: &str) -> PingResult {
    let mut stream = TcpStream::connect(&host).unwrap();
    let req = PingRequest {};
    let req_bytes = bincode::serialize(&req)
        .map_err(|e| Error::SerializationError(format!("{:?}", e)))
        .unwrap();
    write_packet(&mut stream, HT_PING_REQUEST, &req_bytes).unwrap();
    let (header, res_bytes) = &read_packets(&mut stream, 1).unwrap()[0];
    assert_eq!(header.ty, HT_PING_RESULT);
    bincode::deserialize(&res_bytes)
        .map_err(|e| Error::SerializationError(format!("{:?}", e)))
        .unwrap()
}

/// Sends a compute request to the given host and returns the result.
pub fn send_compute(host: &str, req: ComputeRequest) -> ComputeResult {
    let mut stream = TcpStream::connect(&host).unwrap();
    let req_bytes = bincode::serialize(&req)
        .map_err(|e| Error::SerializationError(format!("{:?}", e)))
        .unwrap();
    write_packet(&mut stream, HT_COMPUTE_REQUEST, &req_bytes).unwrap();
    let (header, res_bytes) = &read_packets(&mut stream, 1).unwrap()[0];
    assert_eq!(header.ty, HT_COMPUTE_RESULT);
    bincode::deserialize(&res_bytes)
        .map_err(|e| Error::SerializationError(format!("{:?}", e)))
        .unwrap()
}
