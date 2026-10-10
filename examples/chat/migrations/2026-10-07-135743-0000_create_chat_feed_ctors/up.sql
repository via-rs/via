CREATE TYPE user_preview AS (
    id uuid,
    email text,
    username text
);

CREATE FUNCTION react_to(
    in_thread uuid,
    actor_id uuid,
    val varchar
) RETURNS TABLE (
    id uuid,
    emoji varchar,
    thread_id uuid,
    user_id uuid,
    created_at timestamptz,
    updated_at timestamptz,

    uid uuid,
    email text,
    username text
)
LANGUAGE plpgsql VOLATILE
AS $$
BEGIN
    RETURN QUERY
    WITH inserted AS (
        INSERT INTO reactions (thread_id, user_id, emoji)
        SELECT in_thread, actor_id, val
        FROM threads AS t
        -- actor_id has a subscription to t.channel_id with claims READ | WRITE
        INNER JOIN subscriptions AS s ON s.channel_id = t.channel_id AND s.user_id = actor_id
        WHERE t.id = in_thread AND has_flags(s.claims, 6)
        RETURNING *
    )
    SELECT
        r.id,
        r.emoji,
        r.thread_id,
        r.user_id,
        r.created_at,
        r.updated_at,
        u.id,
        u.email,
        u.username
    FROM inserted AS r
    INNER JOIN users AS u ON u.id = r.user_id;
END;
$$;

CREATE FUNCTION reply_to(
    in_channel uuid,
    parent_id uuid,
    author_id uuid,
    val text
) RETURNS TABLE (
  id uuid,
  body text,
  channel_id uuid,
  thread_id uuid,
  user_id uuid,
  created_at timestamptz,
  updated_at timestamptz,
  total_reactions bigint,
  total_replies bigint,

  uid uuid,
  email text,
  username text
)
LANGUAGE plpgsql VOLATILE
AS $$
BEGIN
    RETURN QUERY
    WITH inserted AS (
        INSERT INTO threads (channel_id, thread_id, user_id, body)
        SELECT in_channel, parent_id, author_id, val
        FROM subscriptions AS s
        -- author_id has a subscription to in_channel with claims READ | WRITE
        WHERE s.channel_id = in_channel AND s.user_id = author_id AND has_flags(s.claims, 6)
        RETURNING *
    )
    SELECT
        t.id,
        t.body,
        t.channel_id,
        t.thread_id,
        t.user_id,
        t.created_at,
        t.updated_at,
        t.total_reactions,
        t.total_replies,
        u.id,
        u.email,
        u.username
    FROM inserted AS t
    INNER JOIN users AS u ON u.id = t.user_id;
END;
$$;
