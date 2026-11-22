-- Every item paired with itself and each of its descendants (a show with its seasons and
-- episodes), kept by triggers. Listings read it instead of walking `parent_id` over the whole
-- library on every query.
CREATE TABLE item_tree (
    root_id INTEGER NOT NULL REFERENCES items(id) ON DELETE CASCADE,
    id      INTEGER NOT NULL REFERENCES items(id) ON DELETE CASCADE,
    PRIMARY KEY (root_id, id)
) WITHOUT ROWID;
CREATE INDEX item_tree_id ON item_tree(id, root_id);

WITH RECURSIVE t(root_id, id) AS (
    SELECT id, id FROM items
    UNION ALL SELECT t.root_id, i.id FROM t JOIN items i ON i.parent_id = t.id
)
INSERT OR IGNORE INTO item_tree(root_id, id) SELECT root_id, id FROM t;

-- A new item is its own root and sits under its parent's ancestors.
CREATE TRIGGER item_tree_ai AFTER INSERT ON items BEGIN
    INSERT OR IGNORE INTO item_tree(root_id, id)
    SELECT root_id, new.id FROM item_tree WHERE id = new.parent_id
    UNION SELECT new.id, new.id;
END;

-- A new parent moves the item's whole subtree: off its old ancestors, under the new ones.
CREATE TRIGGER item_tree_au AFTER UPDATE OF parent_id ON items
WHEN old.parent_id IS NOT new.parent_id BEGIN
    DELETE FROM item_tree
    WHERE id IN (SELECT id FROM item_tree WHERE root_id = new.id)
      AND root_id NOT IN (SELECT id FROM item_tree WHERE root_id = new.id);
    INSERT OR IGNORE INTO item_tree(root_id, id)
    SELECT a.root_id, d.id FROM item_tree a, item_tree d
    WHERE a.id = new.parent_id AND d.root_id = new.id;
END;
