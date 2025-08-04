use std::{collections::HashMap, mem, sync::{Arc, RwLock, Weak}};

use fye_shared::NodeID;
use tokio::sync::{Mutex, OwnedMutexGuard};

#[derive(Clone, Default, Debug)]
pub struct FileWriteLock {
	locked_ids: Arc<RwLock<HashMap<NodeID, Weak<Mutex<()>>>>>,
}

pub struct WriteLockGuard<'l> {
	_write_lock: &'l FileWriteLock,
	_guard: OwnedMutexGuard<()>,
}

impl FileWriteLock {
	fn get_mutex(&self, id: NodeID) -> Arc<Mutex<()>> {
		let locked_ids = self.locked_ids.read().expect("poison");
		if let Some(mutex) = locked_ids.get(&id).and_then(|weak| weak.upgrade()) {
			return mutex;
		}
		
		mem::drop(locked_ids);
		let mut locked_ids = self.locked_ids.write().expect("poison");
		locked_ids.retain(|_, weak| weak.strong_count() > 0);
		
		let mut mutex = None;
		locked_ids.entry(id)
			.and_modify(|weak| mutex = weak.upgrade())
			.or_insert_with(|| {
				let strong = Arc::new(Mutex::new(()));
				let weak = Arc::downgrade(&strong);
				mutex = Some(strong);
				weak
			});
		
		mutex.expect("was set either by upgrading a Weak which had a strong_count higher than 1, or by creating a new Arc")
	}
	
	#[must_use]
	pub async fn lock(&self, id: NodeID) -> WriteLockGuard<'_> {
		let mutex = self.get_mutex(id);
		
		WriteLockGuard {
			_write_lock: self,
			_guard: mutex.lock_owned().await,
		}
	}
}
