// @generated automatically by Diesel CLI.

diesel::table! {
    channels (id) {
        id -> Int8,
        name -> Nullable<Text>,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
    }
}

diesel::table! {
    reactions (id) {
        id -> Int8,
        #[max_length = 16]
        emoji -> Varchar,
        thread_id -> Int8,
        user_id -> Int8,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
    }
}

diesel::table! {
    subscriptions (id) {
        id -> Int8,
        channel_id -> Int8,
        user_id -> Int8,
        claims -> Int4,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
    }
}

diesel::table! {
    threads (id) {
        id -> Int8,
        channel_id -> Int8,
        thread_id -> Nullable<Int8>,
        user_id -> Int8,
        body -> Text,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
        total_reactions -> Int8,
        total_replies -> Int8,
    }
}

diesel::table! {
    users (id) {
        id -> Int8,
        email -> Text,
        username -> Text,
        password_hash -> Text,
        password_changed_at -> Timestamptz,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
    }
}

diesel::joinable!(reactions -> threads (thread_id));
diesel::joinable!(reactions -> users (user_id));
diesel::joinable!(subscriptions -> channels (channel_id));
diesel::joinable!(subscriptions -> users (user_id));
diesel::joinable!(threads -> channels (channel_id));
diesel::joinable!(threads -> users (user_id));

diesel::allow_tables_to_appear_in_same_query!(channels, reactions, subscriptions, threads, users,);
