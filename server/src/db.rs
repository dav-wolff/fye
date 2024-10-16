use diesel::{dsl::{AsSelect, SqlTypeOf}, prelude::*, sqlite::Sqlite};
use diesel::result::Error as DieselError;
use fye_shared::NodeID;

mod schema;
use schema::*;

#[derive(Queryable, Selectable, Debug)]
#[diesel(table_name = node_id)]
#[diesel(check_for_backend(Sqlite))]
pub struct CurrentNodeID {
	pub current_id: i64,
}

#[derive(Queryable, Selectable, Insertable, Debug)]
#[diesel(table_name = directories)]
#[diesel(check_for_backend(Sqlite))]
pub struct Directory {
	pub id: i64,
	pub parent: i64,
}

#[derive(Queryable, Selectable, Insertable, Debug)]
#[diesel(table_name = files)]
#[diesel(check_for_backend(Sqlite))]
pub struct File {
	pub id: i64,
	pub size: i64,
	pub hash: String,
}

#[derive(Queryable, Selectable, Debug)]
#[diesel(table_name = directory_entries)]
#[diesel(check_for_backend(Sqlite))]
pub struct DirectoryEntry {
	pub parent: i64,
	pub name: String,
	pub directory: Option<i64>,
	pub file: Option<i64>,
}

#[derive(Insertable, Debug)]
#[diesel(table_name = directory_entries)]
#[diesel(check_for_backend(Sqlite))]
pub struct NewDirectoryEntry<'a> {
	pub parent: i64,
	pub name: &'a str,
	pub directory: Option<i64>,
	pub file: Option<i64>,
}

// TODO: allowed whilst Directory::children is marked allow(unused)
#[allow(unused)]
pub struct DirectoryChild {
	pub parent: i64,
	pub name: String,
	pub data: EntryKind,
}

// TODO: allowed whilst Directory::children is marked allow(unused)
#[allow(unused)]
pub enum EntryKind {
	File(File),
	Directory(Directory),
}

impl Directory {
	pub fn get(node_id: NodeID) -> directories::BoxedQuery<'static, Sqlite, SqlTypeOf<AsSelect<Self, Sqlite>>> {
		use schema::directories::dsl::*;
		
		directories.filter(id.eq(node_id.0 as i64))
			.select(Directory::as_select())
			.into_boxed()
	}
	
	pub fn exists(conn: &mut SqliteConnection, node_id: NodeID) -> Result<bool, DieselError> {
		match Self::get(node_id).first(conn) {
			Ok(_) => Ok(true),
			Err(DieselError::NotFound) => Ok(false),
			Err(err) => Err(err),
		}
	}
	
	pub fn entries(&self) -> directory_entries::BoxedQuery<'static, Sqlite, SqlTypeOf<AsSelect<DirectoryEntry, Sqlite>>> {
		use schema::directory_entries::dsl::*;
		
		directory_entries.filter(parent.eq(self.id))
			.select(DirectoryEntry::as_select())
			.into_boxed()
	}
	
	// TODO: actually not needed for now, keep it around just in case
	#[allow(unused)]
	pub fn children(&self, conn: &mut SqliteConnection) -> Result<impl Iterator<Item = DirectoryChild>, DieselError> {
		// why does rust-analyzer need a type annotation to know what type this is?
		let entries: Vec<(DirectoryEntry, Option<Directory>, Option<File>)> = directory_entries::table
			.left_join(directories::table.on(directory_entries::parent.eq(directories::id)))
			.left_join(files::table)
			.filter(directory_entries::parent.eq(self.id))
			.select((DirectoryEntry::as_select(), Option::<Directory>::as_select(), Option::<File>::as_select()))
			.load::<(DirectoryEntry, Option<Directory>, Option<File>)>(conn)?;
		
		Ok(entries.into_iter()
			.filter_map(|entry| match entry {
				(entry, Some(directory), None) => Some(DirectoryChild {
					parent: entry.parent,
					name: entry.name,
					data: EntryKind::Directory(directory),
				}),
				(entry, None, Some(file)) => Some(DirectoryChild {
					parent: entry.parent,
					name: entry.name,
					data: EntryKind::File(file),
				}),
				(_, None, None) => None, // TODO: entry for non-existent nodes are skipped, should they be removed?
				(_, Some(_), Some(_)) => panic!("should be impossible due to the check on the directory_entries table"),
			}))
	}
	
	pub fn insert(&self, conn: &mut SqliteConnection) -> Result<(), DieselError> {
		let inserted_rows = diesel::insert_into(directories::table)
			.values(self)
			.execute(conn)?;
		assert_eq!(inserted_rows, 1);
		
		Ok(())
	}
	
	pub fn delete(conn: &mut SqliteConnection, node_id: NodeID) -> Result<bool, DieselError> {
		use schema::directories::dsl::*;
		
		let deleted_rows = diesel::delete(directories.filter(id.eq(node_id.0 as i64)))
			.execute(conn)?;
		assert!(deleted_rows <= 1);
		
		Ok(deleted_rows == 1)
	}
}

impl File {
	pub fn get(node_id: NodeID) -> files::BoxedQuery<'static, Sqlite, SqlTypeOf<AsSelect<Self, Sqlite>>> {
		use schema::files::dsl::*;
		
		files.filter(id.eq(node_id.0 as i64))
			.select(File::as_select())
			.into_boxed()
	}
	
	pub fn exists(conn: &mut SqliteConnection, node_id: NodeID) -> Result<bool, DieselError> {
		match Self::get(node_id).first(conn) {
			Ok(_) => Ok(true),
			Err(DieselError::NotFound) => Ok(false),
			Err(err) => Err(err),
		}
	}
	
	pub fn has_hash(conn: &mut SqliteConnection, node_id: NodeID, expected_hash: &str) -> Result<bool, DieselError> {
		use schema::files::dsl::*;
		
		let result = files.filter(id.eq(node_id.0 as i64).and(hash.eq(expected_hash)))
			.select(File::as_select())
			.first(conn);
		
		match result {
			Ok(_) => Ok(true),
			Err(DieselError::NotFound) => Ok(false),
			Err(err) => Err(err),
		}
	}
	
	pub fn update_content(conn: &mut SqliteConnection, node_id: NodeID, prev_hash: &str, new_hash: &str, new_size: u64) -> Result<bool, DieselError> {
		use schema::files::dsl::*;
		
		let rows_updated = diesel::update(files)
			.filter(id.eq(node_id.0 as i64).and(hash.eq(prev_hash)))
			.set((
				hash.eq(new_hash),
				size.eq(new_size as i64)
			))
			.execute(conn)?;
		
		Ok(rows_updated > 0)
	}
	
	pub fn insert(&self, conn: &mut SqliteConnection) -> Result<(), DieselError> {
		let inserted_rows = diesel::insert_into(files::table)
			.values(self)
			.execute(conn)?;
		assert_eq!(inserted_rows, 1);
		
		Ok(())
	}
	
	pub fn delete(conn: &mut SqliteConnection, node_id: NodeID) -> Result<bool, DieselError> {
		use schema::files::dsl::*;
		
		let deleted_rows = diesel::delete(files.filter(id.eq(node_id.0 as i64)))
			.execute(conn)?;
		assert!(deleted_rows <= 1);
		
		Ok(deleted_rows == 1)
	}
}

impl DirectoryEntry {
	pub fn get(parent_id: NodeID, entry_name: &str) -> directory_entries::BoxedQuery<'_, Sqlite, SqlTypeOf<AsSelect<DirectoryEntry, Sqlite>>> {
		use schema::directory_entries::dsl::*;
		
		directory_entries.filter(parent.eq(parent_id.0 as i64).and(name.eq(entry_name)))
			.select(DirectoryEntry::as_select())
			.into_boxed()
	}
}

impl<'a> NewDirectoryEntry<'a> {
	pub fn insert(&self, conn: &mut SqliteConnection) -> Result<(), DieselError> {
		let inserted_rows = diesel::insert_into(directory_entries::table)
			.values(self)
			.execute(conn)?;
		assert_eq!(inserted_rows, 1);
		
		Ok(())
	}
}

/// Returns the next available [`NodeID`] to use for inserting a new node into the database.
/// 
/// Needs to be used immediately or discarded. If held onto for a long while, it's possible that
/// the same id is given out twice.
/// 
/// # Warning
/// Don't call this function from within a transaction
pub fn next_available_id(conn: &mut SqliteConnection) -> Result<NodeID, DieselError> {
	use schema::node_id::dsl::*;
	
	// infinitely loops if all ids are used up, not likely to occur
	loop {
		let id = diesel::update(node_id)
			.set(current_id.eq(current_id + 1)) // TODO: implement overflow
			.returning(CurrentNodeID::as_returning())
			.get_result(conn)?
			.current_id;
		let id = NodeID(id as u64);
		
		if !File::exists(conn, id)? && !Directory::exists(conn, id)? {
			// potential race condition between the node not existing but being created before whoever requested it uses it
			// as ids are always increasing, this can only occur if the current id wraps around the entire id space
			// should not be an issue
			return Ok(id);
		}
	}
}
