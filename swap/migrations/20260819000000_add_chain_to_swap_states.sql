-- Tag every swap state with the script chain it belongs to.
-- All swaps that existed before this migration were Bitcoin swaps.
ALTER TABLE swap_states ADD COLUMN chain TEXT NOT NULL DEFAULT 'bitcoin';
