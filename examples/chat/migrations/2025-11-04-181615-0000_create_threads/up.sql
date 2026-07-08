CREATE TABLE threads (
  id BIGSERIAL PRIMARY KEY,

  channel_id BIGINT NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
  thread_id BIGINT REFERENCES threads(id) ON DELETE CASCADE,
  user_id BIGINT NOT NULL REFERENCES users(id),

  body TEXT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
  updated_at TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
  total_reactions BIGINT NOT NULL DEFAULT 0,
  total_replies BIGINT NOT NULL DEFAULT 0
);

SELECT diesel_manage_updated_at('threads');

-- Used to list the threads in a channel.
CREATE INDEX threads_recent_by_channel_idx
ON threads (channel_id, created_at DESC, id DESC);

-- Used to list the threads in a thread.
CREATE INDEX threads_recent_by_channel_and_thread_idx
ON threads (channel_id, thread_id, created_at DESC, id DESC);

-- Used to update or delete a user's thread.
CREATE INDEX threads_by_id_and_user_idx
ON threads (id, user_id);

-- Update the total_replies for a thread.
CREATE FUNCTION replies_counter_cache()
RETURNS trigger AS $$
BEGIN
  -- INSERT: increment total_replies on threads
  IF TG_OP = 'INSERT' THEN
    UPDATE threads
    SET total_replies = total_replies + 1
    WHERE id = NEW.thread_id;
    RETURN NEW;

  -- DELETE: decrement total_replies on threads
  ELSIF TG_OP = 'DELETE' THEN
    UPDATE threads
    SET total_replies = total_replies - 1
    WHERE id = OLD.thread_id;
    RETURN OLD;
  END IF;

  RETURN NULL;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER replies_counter_cache_trigger
AFTER INSERT OR DELETE ON threads
FOR EACH ROW
EXECUTE FUNCTION replies_counter_cache();

