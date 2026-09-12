-- Migration 007: Drop the nodes and peers tables.
--
-- The agent-to-agent delegation feature was removed: handing a task to another
-- agent lost the caller's context and turned every remote confirmation into a
-- blind approval, so tasks rarely completed. The node discovery endpoint, the
-- peer probe loop and the UI that managed them went with it.

DROP TABLE IF EXISTS nodes;
DROP INDEX IF EXISTS idx_nodes_name;
DROP TABLE IF EXISTS peers;
DROP INDEX IF EXISTS idx_peers_name;
DROP INDEX IF EXISTS idx_peers_enabled;
