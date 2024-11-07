use std::time::Duration;

fn start_background_task() {
    tokio::spawn(async {
        let mut count = 1;
        loop {
            println!("Do some background task: {}", count);
            count += 1;
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    });
}

pub(crate) fn main() {
    tokio::spawn(async {
        start_background_task();
    });
}
