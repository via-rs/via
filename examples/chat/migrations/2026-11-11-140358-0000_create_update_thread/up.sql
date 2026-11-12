CREATE FUNCTION update_thread(
    target_id uuid,
    author_id uuid,
    new_body text
) RETURNS TABLE (
  did_change boolean,

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
        UPDATE threads
        SET body = new_body
        WHERE threads.id = target_id AND threads.user_id = author_id
        RETURNING
          *,
          OLD.body IS DISTINCT FROM NEW.body AS did_change
    )
    SELECT
        t.did_change,
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
