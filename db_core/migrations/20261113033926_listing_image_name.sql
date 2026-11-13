-- #122: an optional host-set name per listing photo, shown in the suite photo
-- picker and used as guest alt text and full-size viewer caption. NULL means
-- unnamed; the portals show "Photo N" (by position) instead.
ALTER TABLE listing_image ADD COLUMN name VARCHAR(80);
