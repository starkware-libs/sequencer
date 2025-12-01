use std::io::{self, Write};

use apollo_infra::trace_util::{configure_tracing, configure_tracing_new};
use apollo_infra_utils::set_global_allocator;
use tracing::{error, info, instrument, warn, Level};

set_global_allocator!();

#[instrument(err, ret)]
pub fn temp_fn(input: u8) -> Result<u8, u8> {
    if input > 5 {
        info!("ok path");
        Ok(4)
    } else {
        error!("error path");
        Err(8)
    }
}

#[instrument()]
pub fn temp_fn_with_warn(input: u8) -> Result<u8, u8> {
    if input > 5 {
        info!("ok path");
        Ok(4)
    } else {
        error!("error path");
        Err(8)
    }
}

/// #[instrument(err(level = Level::INFO))]

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Prompt the user for input
    print!("Please enter a number (u8): ");
    io::stdout().flush()?; // Ensure the prompt is displayed immediately

    let mut user_input = String::new();
    io::stdin().read_line(&mut user_input)?;

    // Trim whitespace and parse the input into a u8
    let number: u8 = match user_input.trim().parse() {
        Ok(num) => num,
        Err(_) => {
            eprintln!("Error: Invalid input. Please enter a valid non-negative integer.");
            return Ok(()); // Exit gracefully if parsing fails
        }
    };

    if number == 1_u8 {
        configure_tracing().await;
    } else {
        configure_tracing_new().await;
    }

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
