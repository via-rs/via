CREATE TABLE subscriptions (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

  channel_id UUID NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
  user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,

  claims INTEGER NOT NULL DEFAULT 1,
  created_at TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
  updated_at TIMESTAMPTZ NOT NULL DEFAULT current_timestamp
);

-- Automated timestamp updates managed by diesel.
SELECT diesel_manage_updated_at('subscriptions');

-- Asserts that a user cannot have more than one subscription to a channel.
CREATE UNIQUE INDEX subscriptions_by_join_idx
ON subscriptions (user_id, channel_id);

-- Used to list the users in a channel.
CREATE INDEX subscriptions_recent_by_channel_idx
ON subscriptions(channel_id, created_at DESC, id DESC);

-- Used to list a user's channels.
CREATE INDEX subscriptions_recent_by_user_idx
ON subscriptions (user_id, channel_id, created_at DESC, id DESC);

-- Used to filter subscriptions that have the base level of permissions
-- required to participate in a channel.
--
-- VIEW | WRITE | REACT
--
CREATE INDEX subscriptions_claims_can_participate_idx
ON subscriptions((claims & 7));

-- Build a BitAnd Eq expression from diesel's query DSL.
-- Used to assert auth claims that a user has in a channel.
CREATE FUNCTION has_flags(lhs integer, rhs integer)
RETURNS boolean AS $$
  SELECT (lhs & rhs) = rhs;
$$ LANGUAGE SQL IMMUTABLE;
