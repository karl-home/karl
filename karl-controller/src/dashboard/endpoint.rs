use rocket::http::Status;
use rocket_contrib::json::Json;

#[allow(non_snake_case)]
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct GraphJson {
    // sensors indexed 0 to n-1, where n is the number of sensors
    sensors: Vec<SensorJson>,
    // modules indexed n to n+m-1, where m is the number of modules
    moduleIds: Vec<ModuleJson>,
    // stateless, out_id, out_red, module_id, module_param
    dataEdges: Vec<(bool, u32, u32, u32, u32)>,
    // module_id, module_ret, sensor_id, sensor_key
    stateEdges: Vec<(u32, u32, u32, u32)>,
    // module_id, domain
    networkEdges: Vec<(u32, String)>,
    // module_id, duration_s
    intervals: Vec<(u32, u32)>,
}

#[allow(non_snake_case)]
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SensorJson {
    id: String,
    stateKeys: Vec<(String, String)>,
    returns: Vec<(String, String)>,
}

#[allow(non_snake_case)]
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ModuleJson {
    localId: String,
    globalId: String,
    params: Vec<String>,
    returns: Vec<String>,
}

#[get("/graph")]
pub fn get_graph() -> Result<Json<GraphJson>, Status> {
    // TODO: unimplemented
    info!("get_graph");
    Ok(Json(GraphJson::default()))
}

#[post("/graph", format = "json", data = "<graph>")]
pub fn save_graph(graph: Json<GraphJson>) -> Status {
    info!("save_graph");
    info!("{:?}", graph);
    Status::NotImplemented
}

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
