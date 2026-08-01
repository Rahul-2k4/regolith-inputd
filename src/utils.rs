use log::{info, warn};
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

pub fn is_supported_input_type(input_type: &str) -> bool {
    matches!(input_type, "pointer" | "keyboard" | "touchpad")
}

pub fn sync_input_settings(
    handlers_sref: &mut HandlerList,
    input: &Input,
) -> Result<(), Box<dyn Error>> {
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

pub fn retry_action<F, T, E>(
    action: F,
    max_retry: usize,
    duration_before_retry: Duration,
) -> Result<T, E>
where
    F: FnMut() -> Result<T, E>,
    E: Display,
{
    retry_fallible(action, max_retry, duration_before_retry)
}

pub fn retry_fallible<F, T, E>(
    mut action: F,
    max_retry: usize,
    duration_before_retry: Duration,
) -> Result<T, E>
where
    F: FnMut() -> Result<T, E>,
    E: Display,
{
    for attempt in 0..=max_retry {
        match action() {
            Ok(result) => return Ok(result),
            Err(error) if attempt == max_retry => return Err(error),
            Err(error) => {
                warn!("{error}");
                thread::sleep(duration_before_retry);
            }
        }
    }
    unreachable!()
}

#[cfg(test)]
mod tests {
    use super::{is_supported_input_type, recover_lock, retry_action, retry_fallible};
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;

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

    #[test]
    fn recognizes_only_supported_input_types() {
        assert!(is_supported_input_type("pointer"));
        assert!(is_supported_input_type("keyboard"));
        assert!(is_supported_input_type("touchpad"));
        assert!(!is_supported_input_type("tablet"));
    }

    #[test]
    fn retries_after_failure() {
        let mut attempts = 0;
        let result = retry_fallible(
            || {
                attempts += 1;
                if attempts == 3 {
                    Ok(attempts)
                } else {
                    Err("temporary failure")
                }
            },
            3,
            Duration::ZERO,
        );
        assert_eq!(result, Ok(3));
        assert_eq!(attempts, 3);
    }

    #[test]
    fn returns_final_error_after_retries() {
        let mut attempts = 0;
        let result: Result<(), &str> = retry_fallible(
            || {
                attempts += 1;
                Err("final failure")
            },
            2,
            Duration::ZERO,
        );
        assert_eq!(result.unwrap_err(), "final failure");
        assert_eq!(attempts, 3);
    }

    #[test]
    fn returns_final_error_without_panicking_after_retries() {
        let mut attempts = 0;
        let result = retry_action(
            || {
                attempts += 1;
                Err::<(), _>("final failure")
            },
            2,
            Duration::ZERO,
        );

        assert_eq!(result, Err("final failure"));
        assert_eq!(attempts, 3);
    }
}
