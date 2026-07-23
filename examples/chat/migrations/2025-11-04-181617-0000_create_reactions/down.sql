DROP TRIGGER reactions_counter_cache_trigger ON reactions;

DROP FUNCTION top_reactions_for;
DROP FUNCTION reactions_counter_cache;

DROP INDEX reactions_recent_by_thread_idx;
DROP INDEX reactions_distinct_by_thread_idx;
DROP INDEX reactions_distinct_by_thread_sorted_idx;

DROP TABLE reactions;
