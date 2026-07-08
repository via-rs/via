DROP TRIGGER replies_counter_cache_trigger ON threads;

DROP FUNCTION replies_counter_cache;

DROP INDEX threads_by_id_and_user_idx;
DROP INDEX threads_recent_by_channel_and_thread_idx;
DROP INDEX threads_recent_by_channel_idx;

DROP TABLE threads;
