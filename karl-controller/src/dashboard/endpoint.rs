use rocket::http::Status;

#[get("/graph")]
pub fn get_graph() -> Result<Vec<u8>, Status> {
    Ok(vec![])
}

// export function getGraph(): GraphFormat {
// console.error('unimplemented: get graph from mock network')
// return undefined
// }

// // POST /graph (data: graph)
// export function saveGraph(format: GraphFormat) {
// console.error('unimplemented: save graph to mock network')
// console.log(format)
// }

// // POST /module/<id>
// export function spawnModule(moduleId: string) {
// console.error(`unimplemented: spawn module ${moduleId}`)
// }

// // POST /host/confirm/<id>
// export function confirmHost(hostId: string) {
// console.error(`unimplemented: confirm host in mock network ${hostId}`)
// }

// // POST /host/cancel/<id>
// export function cancelHost(hostId: string) {
// console.error(`unimplemented: cancel host in mock network ${hostId}`)
// }

// // POST /sensor/confirm/<id>
// export function confirmSensor(sensorId: string) {
// console.error(`unimplemented: confirm sensor in mock network ${sensorId}`)
// }

// // POST /sensor/cancel/<id>
// export function cancelSensor(sensorId: string) {
// console.error(`unimplemented: cancel sensor in mock network ${sensorId}`)
// }

// // GET /sensors
// export function getSensors(): { sensor: Sensor, attestation: string }[] {
// return []
// }

// // GET /hosts
// export function getHosts(): { confirmed: Host[], unconfirmed: string[] } {
// return { confirmed: [], unconfirmed: [] }

// #[post("/confirm/host/<service_name>")]
// fn confirm_host(
//     host_header: HostHeader,
//     service_name: String,
// ) {
//     // sdfasdfa
// }