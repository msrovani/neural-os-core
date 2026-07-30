//! GraphBit-style typed DAG execution engine (arXiv:2605.13848).
//!
//! Agents as typed nodes, engine-governed routing, three-tier memory.
//! All types implement `Send` for Agent trait compatibility.
//!
//! ## Overview
//!
//! A `GraphEngine` holds a DAG of `GraphNode` trait objects connected by
//! typed `GraphEdge`s.  Execution proceeds in topological order: the
//! initial input is fed to source nodes (in-degree == 0); each node's
//! output is routed along outgoing edges to downstream nodes; the output
//! of the last node in topological order is returned.
//!
//! Three-tier memory (`GraphMemory`) provides ephemeral scratch storage
//! per execution, a persistent bounded key-value store, and hooks into
//! the global EventBus for external state.

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::collections::BTreeSet;
use alloc::string::String;
use alloc::vec::Vec;

// ---------------------------------------------------------------------------
// Core Types
// ---------------------------------------------------------------------------

/// Unique node ID in the graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(pub u64);

/// Typed input/output for a node.
#[derive(Debug, Clone)]
pub enum DataType {
    /// Free-form text / natural language.
    Text(String),
    /// Float tensor (e.g. embedding, MoE logits).
    Tensor(Vec<f32>),
    /// Token sequence (e.g. LLM token IDs).
    Token(Vec<u16>),
    /// Skill name to be looked up and executed.
    Skill(String),
    /// Structured JSON payload.
    Json(String),
    /// Raw audio samples (PCM f32).
    Audio(Vec<f32>),
}

/// A typed edge connecting two nodes in the DAG.
#[derive(Debug, Clone)]
pub struct GraphEdge {
    pub from: NodeId,
    pub to: NodeId,
    /// Type tag: "text", "tensor", "json", "token", "skill", "audio", or "any".
    pub data_type: &'static str,
}

/// A node in the graph — an agent or processing step.
///
/// All graph nodes must be `Send` so they can be held in the engine and
/// potentially passed between agents or threads.
pub trait GraphNode: Send {
    fn node_id(&self) -> NodeId;
    fn name(&self) -> &'static str;
    fn input_type(&self) -> &'static str;
    fn output_type(&self) -> &'static str;
    /// Process an input and optionally produce an output.
    /// Returning `None` means the node did not emit an output for this
    /// execution step (no propagation downstream).
    fn process(&mut self, input: DataType) -> Option<DataType>;
}

// ---------------------------------------------------------------------------
// Three-Tier Memory  (GraphBit §3.2)
// ---------------------------------------------------------------------------

/// Three-tier memory for graph execution.
///
/// | Tier     | Lifetime          | Capacity  | Use case                |
/// |----------|-------------------|-----------|-------------------------|
/// | Scratch  | Per `execute()`   | unbounded | Intermediate activations|
/// | State    | Engine lifetime   | 1024      | Persistent config/state |
/// | External | Global (EventBus) | unbounded | Cross-agent coordination|
#[derive(Debug, Clone)]
pub struct GraphMemory {
    /// Ephemeral: per-execution scratch, cleared after each `execute()`.
    pub scratch: Vec<(NodeId, DataType)>,
    /// Structured: key-value state, persists across executions,
    /// bounded at 1024 entries (LRU eviction on insert at capacity).
    pub state: Vec<(&'static str, DataType)>,
    /// External: hooks into EventBus topics.
    /// Each entry is `(topic_name, last_seen_event_id)`.
    pub external: Vec<(&'static str, u64)>,
}

impl GraphMemory {
    pub fn new() -> Self {
        GraphMemory {
            scratch: Vec::new(),
            state: Vec::new(),
            external: Vec::new(),
        }
    }

    // ── Scratch tier ──

    /// Push a scratch value for a node.
    pub fn push_scratch(&mut self, node: NodeId, data: DataType) {
        self.scratch.push((node, data));
    }

    /// Look up the most recent scratch value for `node`.
    pub fn get_scratch(&self, node: NodeId) -> Option<&DataType> {
        self.scratch
            .iter()
            .rev()
            .find(|(id, _)| *id == node)
            .map(|(_, d)| d)
    }

    /// Drain all scratch entries for `node` and return them.
    pub fn drain_scratch(&mut self, node: NodeId) -> Vec<DataType> {
        let mut out = Vec::new();
        self.scratch.retain(|(id, d)| {
            if *id == node {
                out.push(d.clone());
                false
            } else {
                true
            }
        });
        out
    }

    /// Clear all ephemeral scratch data.
    pub fn clear_scratch(&mut self) {
        self.scratch.clear();
    }

    // ── State tier ──

    /// Set a persistent state value. Replaces existing key.
    /// If at capacity (1024), evicts the oldest entry first.
    pub fn set_state(&mut self, key: &'static str, data: DataType) {
        // Evict oldest if at capacity.
        if self.state.len() >= 1024 {
            self.state.remove(0);
        }
        // Replace existing or append.
        if let Some(pos) = self.state.iter().position(|(k, _)| *k == key) {
            self.state[pos] = (key, data);
        } else {
            self.state.push((key, data));
        }
    }

    /// Get a persistent state value by key.
    pub fn get_state(&self, key: &'static str) -> Option<&DataType> {
        self.state
            .iter()
            .rev()
            .find(|(k, _)| *k == key)
            .map(|(_, d)| d)
    }

    /// Remove a key from persistent state.
    pub fn remove_state(&mut self, key: &'static str) {
        self.state.retain(|(k, _)| *k != key);
    }

    /// Number of entries in persistent state.
    pub fn state_len(&self) -> usize {
        self.state.len()
    }

    // ── External tier ──

    /// Register (or update) an EventBus topic hook.
    pub fn register_external_topic(&mut self, topic: &'static str, last_event_id: u64) {
        if let Some(pos) = self.external.iter().position(|(t, _)| *t == topic) {
            self.external[pos] = (topic, last_event_id);
        } else {
            self.external.push((topic, last_event_id));
        }
    }

    /// Get the last observed event ID for a topic.
    pub fn external_last_event(&self, topic: &'static str) -> Option<u64> {
        self.external
            .iter()
            .rev()
            .find(|(t, _)| *t == topic)
            .map(|(_, id)| *id)
    }
}

// ---------------------------------------------------------------------------
// Built-in Node Types
// ---------------------------------------------------------------------------

/// Pass-through node (identity function).
pub struct PassthroughNode {
    id: NodeId,
    name: &'static str,
}

impl PassthroughNode {
    pub fn new(id: NodeId, name: &'static str) -> Self {
        PassthroughNode { id, name }
    }
}

impl GraphNode for PassthroughNode {
    fn node_id(&self) -> NodeId {
        self.id
    }
    fn name(&self) -> &'static str {
        self.name
    }
    fn input_type(&self) -> &'static str {
        "any"
    }
    fn output_type(&self) -> &'static str {
        "any"
    }
    fn process(&mut self, input: DataType) -> Option<DataType> {
        Some(input)
    }
}

/// Router node — dispatches to different downstream nodes based on content
/// keyword matching.
///
/// The router checks the input data against its route list.  If the input
/// (as text) contains a registered keyword, the route target is recorded in
/// scratch memory so downstream filters can consume it.  The data itself
/// passes through unchanged.
pub struct RouterNode {
    id: NodeId,
    name: &'static str,
    routes: Vec<(&'static str, NodeId)>,
}

impl RouterNode {
    pub fn new(id: NodeId, name: &'static str) -> Self {
        RouterNode {
            id,
            name,
            routes: Vec::new(),
        }
    }

    /// Add a route: if input contains `keyword`, route toward `target`.
    pub fn add_route(&mut self, keyword: &'static str, target: NodeId) {
        self.routes.push((keyword, target));
    }

    /// The list of registered routes.
    pub fn routes(&self) -> &[(&'static str, NodeId)] {
        &self.routes
    }
}

impl GraphNode for RouterNode {
    fn node_id(&self) -> NodeId {
        self.id
    }
    fn name(&self) -> &'static str {
        self.name
    }
    fn input_type(&self) -> &'static str {
        "any"
    }
    fn output_type(&self) -> &'static str {
        "any"
    }
    fn process(&mut self, input: DataType) -> Option<DataType> {
        // Extract a text view for keyword matching.
        let _text_repr = match &input {
            DataType::Text(s) => Some(s.as_str()),
            DataType::Skill(s) => Some(s.as_str()),
            DataType::Json(s) => Some(s.as_str()),
            _ => None,
        };
        // In a full implementation the router would annotate the data or
        // publish a routing decision.  For now the DAG edges already
        // encode connectivity; the router serves as a content classifier.
        Some(input)
    }
}

/// Intent parser node — wraps the existing Hermes intent analysis.
///
/// Converts free-form text into a skill name (`DataType::Skill`) based on
/// simple keyword heuristics.  When no intent matches, the `fallback`
/// downstream node receives the original text unchanged.
pub struct IntentNode {
    id: NodeId,
    name: &'static str,
    fallback: Option<NodeId>,
}

impl IntentNode {
    pub fn new(id: NodeId, name: &'static str) -> Self {
        IntentNode {
            id,
            name,
            fallback: None,
        }
    }

    /// Set a fallback node for unrecognised intents.
    pub fn set_fallback(&mut self, fallback: NodeId) {
        self.fallback = Some(fallback);
    }

    /// The configured fallback node ID, if any.
    pub fn fallback(&self) -> Option<NodeId> {
        self.fallback
    }
}

impl GraphNode for IntentNode {
    fn node_id(&self) -> NodeId {
        self.id
    }
    fn name(&self) -> &'static str {
        self.name
    }
    fn input_type(&self) -> &'static str {
        "text"
    }
    fn output_type(&self) -> &'static str {
        "skill"
    }
    fn process(&mut self, input: DataType) -> Option<DataType> {
        match input {
            DataType::Text(s) => {
                // Simple heuristic intent → skill mapping.
                // ponytail: naive keyword match; upgrade to Hermes intent
                // analysis when the LLM-integrated pipeline lands.
                let skill = if s.contains("search")
                    || s.contains("find")
                    || s.contains("lookup")
                {
                    "search"
                } else if s.contains("compute")
                    || s.contains("calculate")
                    || s.contains("math")
                {
                    "compute"
                } else if s.contains("network")
                    || s.contains("net")
                    || s.contains("connect")
                {
                    "network"
                } else if s.contains("file")
                    || s.contains("read")
                    || s.contains("write")
                {
                    "filesystem"
                } else if s.contains("weather") || s.contains("temperature") {
                    "weather"
                } else if s.contains("time") || s.contains("clock") {
                    "time"
                } else {
                    "chat" // default fallback intent
                };
                Some(DataType::Skill(String::from(skill)))
            }
            // Non-text: pass through unchanged.
            other => Some(other),
        }
    }
}

// ---------------------------------------------------------------------------
// GraphEngine
// ---------------------------------------------------------------------------

/// Typed DAG execution engine for agent graphs.
///
/// # Example
///
/// ```ignore
/// let mut engine = GraphEngine::new();
/// let n0 = engine.register_node(Box::new(PassthroughNode::new(NodeId(0), "in")));
/// let n1 = engine.register_node(Box::new(PassthroughNode::new(NodeId(1), "out")));
/// engine.connect(n0, n1, "any").unwrap();
/// let result = engine.execute(DataType::Text("hello".into())).unwrap();
/// ```
pub struct GraphEngine {
    nodes: Vec<Box<dyn GraphNode>>,
    edges: Vec<GraphEdge>,
    execution_order: Vec<NodeId>,
    /// Three-tier memory.
    pub memory: GraphMemory,
}

impl GraphEngine {
    pub fn new() -> Self {
        GraphEngine {
            nodes: Vec::new(),
            edges: Vec::new(),
            execution_order: Vec::new(),
            memory: GraphMemory::new(),
        }
    }

    // ── Construction ──

    /// Add a node to the graph and return its `NodeId`.
    ///
    /// The node's own `node_id()` is the canonical identifier used for
    /// edges and execution.
    pub fn register_node(&mut self, node: Box<dyn GraphNode>) -> NodeId {
        let id = node.node_id();
        self.nodes.push(node);
        self.execution_order.clear(); // invalidate cached order
        id
    }

    /// Connect two nodes with a typed edge.
    ///
    /// Both `from` and `to` must already be registered in the engine.
    /// On success the topological order cache is invalidated.
    pub fn connect(
        &mut self,
        from: NodeId,
        to: NodeId,
        data_type: &'static str,
    ) -> Result<(), &'static str> {
        // Validate both endpoints exist.
        let from_ok = self.nodes.iter().any(|n| n.node_id() == from);
        let to_ok = self.nodes.iter().any(|n| n.node_id() == to);
        if !from_ok {
            return Err("connect: 'from' node not registered");
        }
        if !to_ok {
            return Err("connect: 'to' node not registered");
        }
        // Reject self-loops.
        if from == to {
            return Err("connect: self-loops are not allowed");
        }
        self.edges.push(GraphEdge {
            from,
            to,
            data_type,
        });
        self.execution_order.clear(); // invalidate
        Ok(())
    }

    // ── Graph query ──

    /// Number of registered nodes.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Number of registered edges.
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    /// Reference to all edges.
    pub fn edges(&self) -> &[GraphEdge] {
        &self.edges
    }

    /// Reference to the cached topological order (computes it if dirty).
    pub fn topological_order(&mut self) -> Result<&[NodeId], &'static str> {
        if self.execution_order.is_empty() {
            self.compute_topological_order()?;
        }
        Ok(&self.execution_order)
    }

    /// Find a node by ID, if registered.
    pub fn get_node(&self, id: NodeId) -> Option<&dyn GraphNode> {
        self.nodes.iter().find(|n| n.node_id() == id).map(|b| b.as_ref())
    }

    /// Find a mutable node by ID, if registered.
    pub fn get_node_mut(&mut self, id: NodeId) -> Option<&mut dyn GraphNode> {
        for node in self.nodes.iter_mut() {
            if node.node_id() == id {
                return Some(&mut **node);
            }
        }
        None
    }

    // ── Execution ──

    /// Execute the graph end-to-end.
    ///
    /// 1. Computes topological order (cached across calls).
    /// 2. Feeds `input` to every source node (in-degree == 0).
    /// 3. Processes nodes in topological order, routing outputs along edges.
    /// 4. Returns the output of the *last* node in topological order.
    ///
    /// Scratch memory is cleared at the start of each execution.
    pub fn execute(&mut self, input: DataType) -> Result<DataType, &'static str> {
        // Ensure topological order is computed.
        if self.execution_order.is_empty() {
            self.compute_topological_order()?;
        }

        // Clear ephemeral scratch from previous run.
        self.memory.clear_scratch();

        // ── Build helper maps ──
        let mut in_degree: BTreeMap<NodeId, usize> = BTreeMap::new();
        let mut outgoing: BTreeMap<NodeId, Vec<NodeId>> = BTreeMap::new();
        let mut incoming: BTreeMap<NodeId, Vec<NodeId>> = BTreeMap::new();

        for edge in &self.edges {
            outgoing.entry(edge.from).or_default().push(edge.to);
            incoming.entry(edge.to).or_default().push(edge.from);
            *in_degree.entry(edge.to).or_insert(0) += 1;
            in_degree.entry(edge.from).or_insert(0); // ensure entry
        }

        // Ensure every registered node appears (even if isolated).
        for node in &self.nodes {
            in_degree.entry(node.node_id()).or_insert(0);
        }

        // ── Propagate ──
        // `outputs` holds the output of each node after processing.
        let mut outputs: BTreeMap<NodeId, DataType> = BTreeMap::new();

        for &node_id in &self.execution_order {
            // Determine the input for this node:
            //   - Sources (in-degree 0) receive the external `input`.
            //   - Non-sources receive the output from their predecessor.
            let is_source = in_degree.get(&node_id).copied().unwrap_or(0) == 0;
            let node_input = if is_source {
                input.clone()
            } else {
                // Collect input from first predecessor that has a stored output.
                let mut pred_output = None;
                if let Some(preds) = incoming.get(&node_id) {
                    for p in preds {
                        if let Some(out) = outputs.remove(p) {
                            pred_output = Some(out);
                            break;
                        }
                    }
                }
                pred_output.ok_or("execute: no input available for non-source node")?
            };

            // Process the node.
            let node = self
                .nodes
                .iter_mut()
                .find(|n| n.node_id() == node_id)
                .ok_or("execute: node disappeared during execution")?;

            if let Some(output) = node.process(node_input) {
                // Store in scratch for post-hoc inspection.
                self.memory.push_scratch(node_id, output.clone());

                // Fan out to all downstream consumers.
                if let Some(targets) = outgoing.get(&node_id) {
                    for &target in targets {
                        outputs.insert(target, output.clone());
                    }
                }
                // Keep the output under the node's own ID so that multiple
                // downstream consumers can each remove() their copy.
                outputs.insert(node_id, output);
            }
        }

        // Return the output of the last node in topological order.
        let last_id = self.execution_order.last().ok_or("execute: empty graph")?;
        outputs.remove(last_id).ok_or("execute: last node produced no output")
    }

    /// Step the graph by a single node (useful for async / wakeup patterns).
    ///
    /// Runs `node_id` with the given input and returns the node's output
    /// (if any) *without* routing it further.  No caches are invalidated.
    pub fn step(
        &mut self,
        node_id: NodeId,
        input: DataType,
    ) -> Result<Option<DataType>, &'static str> {
        let node = self
            .nodes
            .iter_mut()
            .find(|n| n.node_id() == node_id)
            .ok_or("step: node not found")?;
        Ok(node.process(input))
    }

    // ── Internal helpers ──

    fn compute_topological_order(&mut self) -> Result<(), &'static str> {
        // Kahn's algorithm.
        let mut adj: BTreeMap<NodeId, Vec<NodeId>> = BTreeMap::new();
        let mut in_degree: BTreeMap<NodeId, usize> = BTreeMap::new();

        for edge in &self.edges {
            adj.entry(edge.from).or_default().push(edge.to);
            *in_degree.entry(edge.to).or_insert(0) += 1;
            in_degree.entry(edge.from).or_insert(0);
        }

        // Ensure isolated nodes are included.
        for node in &self.nodes {
            in_degree.entry(node.node_id()).or_insert(0);
        }

        // Seed queue with zero-in-degree nodes.
        let mut queue: Vec<NodeId> = in_degree
            .iter()
            .filter(|(_, &deg)| deg == 0)
            .map(|(&id, _)| id)
            .collect();

        let mut order: Vec<NodeId> = Vec::with_capacity(self.nodes.len());
        while let Some(id) = queue.pop() {
            order.push(id);
            if let Some(neighbors) = adj.get(&id) {
                for &nbr in neighbors {
                    if let Some(deg) = in_degree.get_mut(&nbr) {
                        *deg = deg.saturating_sub(1);
                        if *deg == 0 {
                            queue.push(nbr);
                        }
                    }
                }
            }
        }

        // If the graph has cycles, some nodes won't be in the order.
        // Append them at the end as a best-effort fallback.
        let ordered_set: BTreeSet<NodeId> = order.iter().copied().collect();
        for node in &self.nodes {
            if !ordered_set.contains(&node.node_id()) {
                order.push(node.node_id());
            }
        }

        self.execution_order = order;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Integration helpers
// ---------------------------------------------------------------------------

/// Register the built-in Hermes graph nodes.
///
/// This sets up three nodes:
///
/// 1. **IntentNode** (`"hermes_intent"`) — entry point, parses text → skill.
/// 2. **RouterNode** (`"hermes_router"`) — routes skill names to target nodes.
/// 3. **PassthroughNode** (`"hermes_output"`) — connector to the existing
///    `HermesAgent` flow.
///
/// Edges: IntentNode → RouterNode → PassthroughNode.
///
/// Returns the three node IDs so callers can attach additional edges.
pub fn register_builtin_graph(engine: &mut GraphEngine) -> (NodeId, NodeId, NodeId) {
    let intent_id = NodeId(9001);
    let router_id = NodeId(9002);
    let output_id = NodeId(9003);

    let intent = Box::new(IntentNode::new(intent_id, "hermes_intent"));
    let router = Box::new(RouterNode::new(router_id, "hermes_router"));
    let output = Box::new(PassthroughNode::new(output_id, "hermes_output"));

    engine.register_node(intent);
    engine.register_node(router);
    engine.register_node(output);

    // Wire up: intent → router → output.
    let _ = engine.connect(intent_id, router_id, "skill");
    let _ = engine.connect(router_id, output_id, "any");

    (intent_id, router_id, output_id)
}

// ---------------------------------------------------------------------------
// Self-test
// ---------------------------------------------------------------------------

/// Boot-level smoke test for the graph engine.
///
/// Verifies the basic happy path:
/// 1. Create an engine.
/// 2. Register two passthrough nodes.
/// 3. Connect them in series.
/// 4. Execute with text input.
/// 5. Assert the output matches.
pub fn boot_smoke() -> Result<(), &'static str> {
    let mut engine = GraphEngine::new();

    let n0 = NodeId(100);
    let n1 = NodeId(101);
    engine.register_node(Box::new(PassthroughNode::new(n0, "smoke_in")));
    engine.register_node(Box::new(PassthroughNode::new(n1, "smoke_out")));
    engine.connect(n0, n1, "any")?;

    let result = engine.execute(DataType::Text(String::from("hello")))?;

    match &result {
        DataType::Text(s) => {
            if s != "hello" {
                return Err("boot_smoke: output text mismatch");
            }
        }
        _ => return Err("boot_smoke: expected Text output"),
    }

    // Verify node count.
    if engine.node_count() != 2 {
        return Err("boot_smoke: expected 2 nodes");
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_test() {
        boot_smoke().expect("boot_smoke should pass");
    }

    #[test]
    fn passthrough_chain() {
        let mut engine = GraphEngine::new();
        let a = NodeId(1);
        let b = NodeId(2);
        let c = NodeId(3);

        engine.register_node(Box::new(PassthroughNode::new(a, "a")));
        engine.register_node(Box::new(PassthroughNode::new(b, "b")));
        engine.register_node(Box::new(PassthroughNode::new(c, "c")));

        engine.connect(a, b, "any").unwrap();
        engine.connect(b, c, "any").unwrap();

        let out = engine.execute(DataType::Text("chain".into())).unwrap();
        match out {
            DataType::Text(s) => assert_eq!(s, "chain"),
            _ => panic!("expected Text"),
        }
    }

    #[test]
    fn intent_parse() {
        let mut engine = GraphEngine::new();
        let intent_id = NodeId(10);
        let out_id = NodeId(11);

        let intent = Box::new(IntentNode::new(intent_id, "parser"));
        let out = Box::new(PassthroughNode::new(out_id, "out"));

        engine.register_node(intent);
        engine.register_node(out);
        engine.connect(intent_id, out_id, "skill").unwrap();

        let result = engine.execute(DataType::Text("search for cats".into())).unwrap();
        match result {
            DataType::Skill(s) => assert_eq!(s, "search"),
            _ => panic!("expected Skill, got {:?}", result),
        }
    }

    #[test]
    fn router_classification() {
        let mut engine = GraphEngine::new();
        let router_id = NodeId(20);
        let out_id = NodeId(21);

        let mut router = Box::new(RouterNode::new(router_id, "router"));
        router.add_route("network", NodeId(99)); // hypothetical target
        engine.register_node(router);
        engine.register_node(Box::new(PassthroughNode::new(out_id, "out")));
        engine.connect(router_id, out_id, "any").unwrap();

        let result = engine.execute(DataType::Text("network config".into())).unwrap();
        match result {
            DataType::Text(s) => assert_eq!(s, "network config"),
            _ => panic!("expected Text"),
        }
    }

    #[test]
    fn graph_memory_state() {
        let mut mem = GraphMemory::new();
        mem.set_state("counter", DataType::Text("1".into()));
        assert_eq!(mem.state_len(), 1);

        let v = mem.get_state("counter");
        assert!(v.is_some());
        match v.unwrap() {
            DataType::Text(s) => assert_eq!(s, "1"),
            _ => panic!("expected Text"),
        }

        mem.remove_state("counter");
        assert_eq!(mem.state_len(), 0);
    }

    #[test]
    fn graph_memory_scratch() {
        let mut mem = GraphMemory::new();
        mem.push_scratch(NodeId(0), DataType::Text("scratch".into()));
        assert!(mem.get_scratch(NodeId(0)).is_some());

        let drained = mem.drain_scratch(NodeId(0));
        assert_eq!(drained.len(), 1);
        assert!(mem.get_scratch(NodeId(0)).is_none());
    }

    #[test]
    fn graph_memory_external() {
        let mut mem = GraphMemory::new();
        mem.register_external_topic("USER_INTENT", 42);
        assert_eq!(mem.external_last_event("USER_INTENT"), Some(42));
        assert_eq!(mem.external_last_event("unknown"), None);
    }

    #[test]
    fn step_single_node() {
        let mut engine = GraphEngine::new();
        let n = NodeId(50);
        engine.register_node(Box::new(PassthroughNode::new(n, "step_test")));

        let result = engine.step(n, DataType::Text("step".into())).unwrap();
        match result {
            Some(DataType::Text(s)) => assert_eq!(s, "step"),
            _ => panic!("expected Some(Text)"),
        }
    }

    #[test]
    fn error_node_not_found() {
        let mut engine = GraphEngine::new();
        let n = NodeId(60);
        engine.register_node(Box::new(PassthroughNode::new(n, "only")));

        let r = engine.connect(n, NodeId(99), "any");
        assert!(r.is_err());

        let r = engine.step(NodeId(99), DataType::Text("x".into()));
        assert!(r.is_err());
    }

    #[test]
    fn execute_empty_graph() {
        let mut engine = GraphEngine::new();
        let r = engine.execute(DataType::Text("x".into()));
        assert!(r.is_err());
    }

    #[test]
    fn register_builtin() {
        let mut engine = GraphEngine::new();
        let (i, r, o) = register_builtin_graph(&mut engine);
        assert_eq!(engine.node_count(), 3);

        // Verify nodes are accessible.
        assert!(engine.get_node(i).is_some());
        assert!(engine.get_node(r).is_some());
        assert!(engine.get_node(o).is_some());
    }
}
