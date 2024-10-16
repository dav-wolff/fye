// @generated automatically by Diesel CLI.

diesel::table! {
    /// Representation of the `directories` table.
    ///
    /// (Automatically generated by Diesel.)
    directories (id) {
        /// The `id` column of the `directories` table.
        ///
        /// Its SQL type is `BigInt`.
        ///
        /// (Automatically generated by Diesel.)
        id -> BigInt,
        /// The `parent` column of the `directories` table.
        ///
        /// Its SQL type is `BigInt`.
        ///
        /// (Automatically generated by Diesel.)
        parent -> BigInt,
    }
}

diesel::table! {
    /// Representation of the `directory_entries` table.
    ///
    /// (Automatically generated by Diesel.)
    directory_entries (parent, name) {
        /// The `parent` column of the `directory_entries` table.
        ///
        /// Its SQL type is `BigInt`.
        ///
        /// (Automatically generated by Diesel.)
        parent -> BigInt,
        /// The `name` column of the `directory_entries` table.
        ///
        /// Its SQL type is `Text`.
        ///
        /// (Automatically generated by Diesel.)
        name -> Text,
        /// The `directory` column of the `directory_entries` table.
        ///
        /// Its SQL type is `Nullable<BigInt>`.
        ///
        /// (Automatically generated by Diesel.)
        directory -> Nullable<BigInt>,
        /// The `file` column of the `directory_entries` table.
        ///
        /// Its SQL type is `Nullable<BigInt>`.
        ///
        /// (Automatically generated by Diesel.)
        file -> Nullable<BigInt>,
    }
}

diesel::table! {
    /// Representation of the `files` table.
    ///
    /// (Automatically generated by Diesel.)
    files (id) {
        /// The `id` column of the `files` table.
        ///
        /// Its SQL type is `BigInt`.
        ///
        /// (Automatically generated by Diesel.)
        id -> BigInt,
        /// The `size` column of the `files` table.
        ///
        /// Its SQL type is `BigInt`.
        ///
        /// (Automatically generated by Diesel.)
        size -> BigInt,
        /// The `hash` column of the `files` table.
        ///
        /// Its SQL type is `Text`.
        ///
        /// (Automatically generated by Diesel.)
        hash -> Text,
    }
}

diesel::table! {
    /// Representation of the `node_id` table.
    ///
    /// (Automatically generated by Diesel.)
    node_id (current_id) {
        /// The `current_id` column of the `node_id` table.
        ///
        /// Its SQL type is `BigInt`.
        ///
        /// (Automatically generated by Diesel.)
        current_id -> BigInt,
    }
}

diesel::joinable!(directory_entries -> files (file));

diesel::allow_tables_to_appear_in_same_query!(
    directories,
    directory_entries,
    files,
    node_id,
);
