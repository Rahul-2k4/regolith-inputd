use log::{error, info, warn};
use std::{
    error::Error,
    fmt::Display,
    sync::{Mutex, MutexGuard},
    thread,
    time::Duration,
};
use swayipc::{Connection as SwayConnection, EventStream, EventType, Fallible, Input};

use crate::HandlerList;

pub fn recover_lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            warn!("Recovering poisoned input handler lock");
            poisoned.into_inner()
        }
    }
}

pub fn sync_input_settings<'a>(
    handlers_sref: &'a mut HandlerList,
    input: &Input,
) -> Result<(), Box<dyn Error + 'a>> {
    let input_type = input.input_type.clone();
    let handler_index = match input_type.as_ref() {
        "pointer" => 0,
        "keyboard" => 1,
        "touchpad" => 2,
        _ => return Err("Incompatible input type".into()),
    };
    info!("Recieved Sway InputEvent for {}", input.input_type);
    let mut handlers_lock = recover_lock(handlers_sref);
    handlers_lock[handler_index].sync_from_sway_input_sync(input)?;
    Ok(())
}

pub fn get_new_inputevent_stream() -> Fallible<EventStream> {
    let connection = SwayConnection::new()?;
    let subs = [EventType::Input, EventType::Tick];
    connection.subscribe(subs)
}

pub fn retry_action<F, T, E>(action: F, max_retry: usize, duration_before_retry: Duration) -> T
where
    F: Fn() -> Result<T, E>,
    E: Display,
{
    let mut retries_left = max_retry;
    loop {
        match action() {
            Ok(res) => break res,
            Err(e) => {
                if retries_left == 0 {
                    error!("{e}");
                    panic!();
                }
                warn!("{e}");
                retries_left -= 1;
                thread::sleep(duration_before_retry);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::recover_lock;
    use std::sync::{Arc, Mutex};
    use std::thread;

    #[test]
    fn recovers_inner_value_from_poisoned_lock() {
        let mutex = Arc::new(Mutex::new(7));
        let poisoned = Arc::clone(&mutex);
        let _ = thread::spawn(move || {
            let _guard = poisoned.lock().unwrap();
            panic!("poison test");
        })
        .join();

        assert_eq!(*recover_lock(&mutex), 7);
    }
}
