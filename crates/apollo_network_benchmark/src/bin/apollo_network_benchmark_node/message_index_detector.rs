#[derive(Default, Clone, Copy)]
pub struct MessageIndexTracker {
    seen_messages_count: u64,
    max_message_index: Option<u64>,
    min_message_index: Option<u64>,
}

impl MessageIndexTracker {
    pub fn seen_message(&mut self, message_index: u64) {
        self.seen_messages_count += 1;
        if self.max_message_index.is_none() || self.max_message_index.unwrap() < message_index {
            self.max_message_index = Some(message_index);
        }
        if self.min_message_index.is_none() || self.min_message_index.unwrap() > message_index {
            self.min_message_index = Some(message_index);
        }
    }

    pub fn pending_messages_count(&self) -> u64 {
        if self.seen_messages_count == 0 {
            return 0;
        }

        let min_message_index = self.min_message_index.unwrap();
        let max_message_index = self.max_message_index.unwrap();
        max_message_index - min_message_index + 1 - self.seen_messages_count
    }
}
