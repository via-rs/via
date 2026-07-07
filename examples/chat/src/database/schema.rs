// @generated automatically by Diesel CLI.

diesel::table! {
    channels (id) {
        id -> BigInt,
        name -> Text,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
    }
}

diesel::table! {
    reactions (id) {
        id -> BigInt,
        #[max_length = 16]
        emoji -> Varchar,
        conversation_id -> BigInt,
        user_id -> BigInt,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
    }
}

diesel::table! {
    subscriptions (id) {
        id -> BigInt,
        channel_id -> BigInt,
        user_id -> BigInt,
        claims -> Int4,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
    }
}

diesel::table! {
    threads (id) {
        id -> BigInt,
        channel_id -> BigInt,
        thread_id -> Nullable<BigInt>,
        user_id -> BigInt,
        body -> Text,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
        total_reactions -> Int8,
        total_replies -> Int8,
    }
}

diesel::table! {
    users (id) {
        id -> BigInt,
        email -> Text,
        username -> Text,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
    }
}

diesel::joinable!(reactions -> threads (conversation_id));
diesel::joinable!(reactions -> users (user_id));
diesel::joinable!(subscriptions -> channels (channel_id));
diesel::joinable!(subscriptions -> users (user_id));
diesel::joinable!(threads -> channels (channel_id));
diesel::joinable!(threads -> users (user_id));

diesel::allow_tables_to_appear_in_same_query!(channels, reactions, subscriptions, threads, users);
