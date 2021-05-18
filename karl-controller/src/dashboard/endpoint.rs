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

#[post("/module/<id>")]
pub fn spawn_module(id: String) -> Status {
    Status::NotImplemented
}

#[allow(non_snake_case)]
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SensorResultJson {
    sensor: SensorJson,
    attestation: String,
}

#[post("/sensor/confirm/<id>")]
pub fn confirm_sensor(id: String) -> Status {
    Status::NotImplemented
}

#[post("/sensor/cancel/<id>")]
pub fn cancel_sensor(id: String) -> Status {
    Status::NotImplemented
}

#[get("/sensors")]
pub fn get_sensors() -> Result<Json<Vec<SensorResultJson>>, Status> {
    Ok(Json(vec![]))
}

#[allow(non_snake_case)]
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct HostResultJson {
    confirmed: Vec<HostJson>,
    unconfirmed: Vec<String>,
}

#[allow(non_snake_case)]
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct HostJson {
    id: String,
    activeModules: u32,
    online: bool,
}

#[post("/host/confirm/<id>")]
pub fn confirm_host(id: String) -> Status {
    Status::NotImplemented
}

#[post("/host/cancel/<id>")]
pub fn cancel_host(id: String) -> Status {
    Status::NotImplemented
}

#[get("/hosts")]
pub fn get_hosts() -> Result<Json<HostResultJson>, Status> {
    Ok(Json(HostResultJson::default()))
}
