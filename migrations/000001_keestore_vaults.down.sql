DROP TRIGGER IF EXISTS vaults_updated_at ON keestore.vaults;
DROP FUNCTION IF EXISTS keestore.set_updated_at();
DROP TABLE IF EXISTS keestore.vaults;
DROP SCHEMA IF EXISTS keestore;
