INSERT INTO candidates (full_name, position, platform)
SELECT 'Avery Morgan', 'President', 'A clearer, more connected student experience.'
WHERE NOT EXISTS (
    SELECT 1 FROM candidates WHERE full_name = 'Avery Morgan' AND position = 'President'
);

INSERT INTO candidates (full_name, position, platform)
SELECT 'Jordan Lee', 'Vice President', 'More student events and stronger campus support.'
WHERE NOT EXISTS (
    SELECT 1 FROM candidates WHERE full_name = 'Jordan Lee' AND position = 'Vice President'
);

INSERT INTO candidates (full_name, position, platform)
SELECT 'Taylor Brooks', 'Secretary', 'Open communication and accessible meeting notes.'
WHERE NOT EXISTS (
    SELECT 1 FROM candidates WHERE full_name = 'Taylor Brooks' AND position = 'Secretary'
);

INSERT INTO candidates (full_name, position, platform)
SELECT 'Riley Chen', 'Treasurer', 'Transparent budgeting for student organizations.'
WHERE NOT EXISTS (
    SELECT 1 FROM candidates WHERE full_name = 'Riley Chen' AND position = 'Treasurer'
);