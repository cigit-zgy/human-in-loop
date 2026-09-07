//! Small first-terminal-wins primitive shared by Ask and structured confirmations.

use std::sync::Mutex;

pub struct FirstTerminalGate<T> {
    value: Mutex<Option<T>>,
}

impl<T> FirstTerminalGate<T> {
    pub fn new() -> Self {
        Self {
            value: Mutex::new(None),
        }
    }

    pub fn try_set(&self, value: T) -> bool {
        let mut current = self.value.lock().unwrap();
        if current.is_some() {
            return false;
        }
        *current = Some(value);
        true
    }

    pub fn is_set(&self) -> bool {
        self.value.lock().unwrap().is_some()
    }

    pub fn with<R>(&self, read: impl FnOnce(Option<&T>) -> R) -> R {
        read(self.value.lock().unwrap().as_ref())
    }
}

impl<T> Default for FirstTerminalGate<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_first_terminal_value_wins() {
        let gate = FirstTerminalGate::new();
        assert!(gate.try_set("first"));
        assert!(!gate.try_set("second"));
        assert_eq!(gate.with(|value| value.copied()), Some("first"));
    }
}
