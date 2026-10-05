-- Remove redundant 'is_admin' attribute from user and user_history attributes JSONB
UPDATE "user"
SET attributes = attributes - 'is_admin'
WHERE attributes ? 'is_admin';

UPDATE "user_history"
SET attributes = attributes - 'is_admin'
WHERE attributes ? 'is_admin';
