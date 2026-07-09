-- Add 'recruiter' to the user role CHECK constraint
ALTER TABLE users DROP CONSTRAINT users_role_check;
ALTER TABLE users ADD CONSTRAINT users_role_check
    CHECK (role IN ('user', 'mentor', 'admin', 'enterprise', 'recruiter'));
