CREATE TABLE channels (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

  name TEXT,
  created_at TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
  updated_at TIMESTAMPTZ NOT NULL DEFAULT current_timestamp
);

SELECT diesel_manage_updated_at('channels');
