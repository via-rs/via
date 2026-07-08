CREATE TABLE channels (
  id BIGSERIAL PRIMARY KEY,

  name TEXT,
  created_at TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
  updated_at TIMESTAMPTZ NOT NULL DEFAULT current_timestamp
);

SELECT diesel_manage_updated_at('channels');
