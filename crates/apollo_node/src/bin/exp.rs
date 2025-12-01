use apollo_infra::trace_util::configure_tracing;
use apollo_infra_utils::set_global_allocator;
use tokio::sync::broadcast::error;
use tracing::{Level, error, info, instrument, warn};

set_global_allocator!();



#[instrument(err,ret)]
pub fn temp_fn(
   input : u8
) -> Result<u8, u8> {
 if input > 5 {
    info!("ok path");
    Ok(4)
 }  else {
    error!("error path");
    Err(8)
 } 
}

#[instrument()]
pub fn temp_fn_with_warn(
   input : u8
) -> Result<u8, u8> {
 if input > 5 {
    info!("ok path");
    Ok(4)
 }  else {
    error!("error path");
    Err(8)
 } 
}


/// #[instrument(err(level = Level::INFO))]





#[tokio::main]
async fn main() -> anyhow::Result<()> {
    configure_tracing().await;

    // info!("this is an info message");
    // warn!("this is an warn message");
    // error!("this is an error message");


    // let ok_val = temp_fn(8);
    // let err_val = temp_fn(2);

    let _ = temp_fn_with_warn(2);

    // info!("this is ok_val: {:?}", ok_val);
    // info!("this is err_val: {:?}", err_val);


    Ok(())
}
