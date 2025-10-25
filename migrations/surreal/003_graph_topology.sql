-- Graph Topology schema migration

DEFINE TABLE graph_node SCHEMAFULL
    PERMISSIONS
        FOR select WHERE tenant_id = $auth.tenant
        OR $auth.role = "admin"
        FOR create, update, delete WHERE $auth.role = "admin";

DEFINE FIELD id           ON graph_node TYPE string;
DEFINE FIELD tenant_id    ON graph_node TYPE string;
DEFINE FIELD kind         ON graph_node TYPE string;
DEFINE FIELD label        ON graph_node TYPE OPTION<string>;
DEFINE FIELD summary      ON graph_node TYPE OPTION<string>;
DEFINE FIELD weight       ON graph_node TYPE OPTION<float>;
DEFINE FIELD metadata     ON graph_node TYPE OPTION<object>;
DEFINE FIELD created_at   ON graph_node TYPE datetime;
DEFINE FIELD updated_at   ON graph_node TYPE datetime;

DEFINE INDEX idx_graph_node_tenant_kind
    ON TABLE graph_node FIELDS tenant_id, kind;

DEFINE INDEX idx_graph_node_updated_at
    ON TABLE graph_node FIELDS updated_at;

DEFINE TABLE graph_edge SCHEMAFULL
    PERMISSIONS
        FOR select WHERE tenant_id = $auth.tenant
        OR $auth.role = "admin"
        FOR create, update, delete WHERE $auth.role = "admin";

DEFINE FIELD id             ON graph_edge TYPE string;
DEFINE FIELD tenant_id      ON graph_edge TYPE string;
DEFINE FIELD from_ref       ON graph_edge TYPE string;
DEFINE FIELD to_ref         ON graph_edge TYPE string;
DEFINE FIELD kind           ON graph_edge TYPE string;
DEFINE FIELD strength       ON graph_edge TYPE OPTION<float>;
DEFINE FIELD confidence     ON graph_edge TYPE OPTION<float>;
DEFINE FIELD temporal_decay ON graph_edge TYPE OPTION<float>;
DEFINE FIELD since_ms       ON graph_edge TYPE OPTION<int>;
DEFINE FIELD until_ms       ON graph_edge TYPE OPTION<int>;
DEFINE FIELD explain        ON graph_edge TYPE OPTION<string>;
DEFINE FIELD properties     ON graph_edge TYPE OPTION<object>;
DEFINE FIELD created_at     ON graph_edge TYPE datetime;
DEFINE FIELD updated_at     ON graph_edge TYPE datetime;

DEFINE INDEX idx_graph_edge_from
    ON TABLE graph_edge FIELDS tenant_id, from_ref, kind;

DEFINE INDEX idx_graph_edge_to
    ON TABLE graph_edge FIELDS tenant_id, to_ref, kind;

DEFINE INDEX idx_graph_edge_kind_strength
    ON TABLE graph_edge FIELDS tenant_id, kind, strength;

DEFINE INDEX idx_graph_edge_time_window
    ON TABLE graph_edge FIELDS tenant_id, kind, since_ms, until_ms;
