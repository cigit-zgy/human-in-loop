// The upstream product updater is not a human-in-loop installation path.
pub fn dispatch(_args: &[String]) -> i32 {
    eprintln!("legacy updater is retired; install an accepted human-in-loop build");
    1
}

#[cfg(test)]
mod tests {
    #[test]
    fn unknown_subcommand_is_rejected() {
        assert_eq!(super::dispatch(&["unknown".to_string()]), 1);
    }
}
