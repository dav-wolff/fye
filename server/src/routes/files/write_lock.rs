use std::{collections::HashSet, mem, sync::Mutex};

use fye_shared::NodeID;
use tokio::sync::Notify;

pub struct WriteLock {
	locked_ids: Mutex<HashSet<NodeID>>,
	notify: Notify,
}

pub struct WriteLockGuard<'l> {
	write_lock: &'l WriteLock,
	locked_id: NodeID,
}

impl WriteLock {
	pub async fn lock(&self, id: NodeID) -> WriteLockGuard<'_> {
		let mut locked_ids = self.locked_ids.lock().expect("poison");
		while locked_ids.contains(&id) {
			mem::drop(locked_ids);
			// race condition
			self.notify.notified().await;
			locked_ids = self.locked_ids.lock().expect("poison");
		}
		
		locked_ids.insert(id);
		
		WriteLockGuard {
			write_lock: self,
			locked_id: id,
		}
	}
	
	fn unlock(&self, id: NodeID) {
		let mut locked_ids = self.locked_ids.lock().expect("poison");
		let did_remove = locked_ids.remove(&id);
		assert!(did_remove, "Tried unlocking id {id} which was not locked");
		self.notify.notify_one();
	}
}

impl<'l> Drop for WriteLockGuard<'l> {
	fn drop(&mut self) {
		self.write_lock.unlock(self.locked_id);
	}
}
