CREATE FUNCTION after_keyset(lhs0 TIMESTAMPTZ, lhs1 UUID, rhs0 TIMESTAMPTZ, rhs1 UUID)
RETURNS boolean AS $$
  SELECT (lhs0, lhs1) > (rhs0, rhs1)
$$ LANGUAGE SQL STABLE;
