use std::error::Error;

async fn _register_hook() -> Result<(), Box<dyn Error>> {
    // TODO: register person_detection
    // TODO: register differential_privacy
    Ok(())
}

async fn _add_edges() -> Result<(), Box<dyn Error>> {
    // TODO: data edge camera.motion -> person_detection
    // TODO: data edge person_detection.count -> differential_privacy
    // TODO: network edge differential_privacy -> https://metrics.karl.zapto.org
    Ok(())
}

async fn _persist_tag() -> Result<(), Box<dyn Error>> {
    // TODO: persist person_detection.box_count
    Ok(())
}

fn main() {
}