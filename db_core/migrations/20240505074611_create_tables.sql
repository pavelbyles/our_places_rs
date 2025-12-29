-- Create listing type table
CREATE TABLE listing_structure (
  id INTEGER PRIMARY KEY,
  structure_name VARCHAR(50) NOT NULL UNIQUE
);

-- Create listing table
CREATE TABLE listing (
  id UUID NOT NULL DEFAULT uuidv7(),
  PRIMARY KEY (id),
  name TEXT NOT NULL UNIQUE,
  description TEXT,
  listing_structure_id INTEGER REFERENCES listing_structure(id) NOT NULL,
  country TEXT NOT NULL,
  price_per_night DECIMAL(10,2),
  is_active BOOLEAN NOT NULL DEFAULT FALSE,
  added_at timestamptz NOT NULL
);

-- Prepopulate listing_structure
INSERT INTO listing_structure (id, structure_name)
VALUES (1, 'Apartment'),
       (2, 'House'),
       (3, 'Townhouse'),
       (4, 'Studio'),
       (5, 'Villa');
