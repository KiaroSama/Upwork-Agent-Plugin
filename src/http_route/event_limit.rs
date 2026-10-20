#[derive(Default)]
pub(super) struct EventLimit {
    retained: usize,
    line: usize,
    comment: bool,
    cr: bool,
}

impl EventLimit {
    pub(super) fn at_boundary(&self) -> bool {
        self.retained == 0 && self.line == 0
    }

    pub(super) fn observe(&mut self, bytes: &[u8], max: usize) -> bool {
        for &byte in bytes {
            if self.cr {
                self.cr = false;
                if byte == b'\n' {
                    continue;
                }
            }
            match byte {
                b'\r' | b'\n' => {
                    if self.line == 0 {
                        self.retained = 0;
                    } else if !self.comment {
                        self.retained = self.retained.saturating_add(self.line).saturating_add(1);
                    }
                    self.line = 0;
                    self.comment = false;
                    self.cr = byte == b'\r';
                }
                _ => {
                    if self.line == 0 {
                        self.comment = byte == b':';
                    }
                    self.line = self.line.saturating_add(1);
                }
            }
            if self.retained.saturating_add(self.line) > max {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limits_split_events_and_retained_multiline_data() {
        let mut limit = EventLimit::default();
        assert!(limit.observe(b"data: abc\r", 11));
        assert!(limit.observe(b"\n\r\n", 11));
        assert!(limit.observe(b"data: abc\n", 11));
        assert!(!limit.observe(b"data: def", 11));
    }

    #[test]
    fn completed_comments_do_not_accumulate_or_reset_data() {
        let mut limit = EventLimit::default();
        for _ in 0..20 {
            assert!(limit.observe(b": ping\n", 8));
        }
        assert!(limit.observe(b"data: a\n", 12));
        assert!(limit.observe(b": x\n", 12));
        assert!(!limit.observe(b"data: b", 12));
    }
}
