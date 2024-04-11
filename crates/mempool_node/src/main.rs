pub mod mempool;
pub mod mempool_proxy;

#[cfg(test)]
mod mempool_proxy_test;

#[tokio::main]
async fn main() {
    let my_string = "Main function placeholder";
    println!("{}", my_string);
}
