//! Forwards a value if a condition is met.
//!
//! Inputs
//! - condition: single 0- or 1-bit
//! - value: the value to pass on if true
//! Outputs
//! - predicate: the original value, as long as the condition is met
//! Network access required
use karl_module_sdk::KarlModuleSDK;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let api = KarlModuleSDK::new();
    let condition = {
        let lower = "2021-09-09T23:56:36.001788121-07:00";
        let upper = chrono::prelude::Local::now().format("%+").to_string();
        api.get("condition", lower, &upper).await?.data
    };
    if !condition.is_empty() {
        let condition = &condition[condition.len() - 1];
        if condition.len() == 1 {
            if condition[0] == 1 {
                println!("condition is met");
                let value = api.get_event("value").await?.unwrap();
                api.push("predicate", value).await?;
            } else {
                println!("condition is not met");
            }
        } else {
            println!("condition is misformed");
        }
    } else {
        println!("condition does not exist")
    }
    Ok(())
}
