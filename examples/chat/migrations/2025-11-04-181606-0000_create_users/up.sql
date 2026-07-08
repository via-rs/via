CREATE TABLE users (
  id BIGSERIAL PRIMARY KEY,

  email TEXT NOT NULL,
  username TEXT NOT NULL,
  password_hash TEXT NOT NULL,
  password_changed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

SELECT diesel_manage_updated_at('users');

CREATE UNIQUE INDEX users_email ON users (email);
CREATE UNIQUE INDEX users_username ON users (username);
