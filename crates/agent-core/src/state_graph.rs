//! StateGraph — scheduler baseado em grafo de estados (LangGraph-inspired).
//! Substitui round-robin: cada agente e um no no grafo, arestas sao
//! condicoes que disparam transicoes. Se nenhuma condicao e satisfeita,
//! o scheduler continua no no atual (pollando o agente).

use alloc::vec::Vec;
use alloc::string::String;

/// Funcao condicional: retorna true se a transicao deve ocorrer
pub type EdgeCondition = fn() -> bool;

pub struct GraphNode {
    pub agent_name: String,
    pub description: &'static str,
}

pub struct GraphEdge {
    pub from: usize,  // indice do no origem
    pub to: usize,    // indice do no destino
    pub condition: EdgeCondition,
    pub label: &'static str,
}

pub struct StateGraph {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub current: Option<usize>,
    pub start_node: usize,
}

impl StateGraph {
    pub fn new() -> Self {
        StateGraph {
            nodes: Vec::new(),
            edges: Vec::new(),
            current: None,
            start_node: 0,
        }
    }

    pub fn add_node(&mut self, name: &str, desc: &'static str) -> usize {
        let idx = self.nodes.len();
        self.nodes.push(GraphNode {
            agent_name: String::from(name),
            description: desc,
        });
        idx
    }

    pub fn add_edge(&mut self, from: usize, to: usize, condition: EdgeCondition, label: &'static str) {
        self.edges.push(GraphEdge { from, to, condition, label });
    }

    /// Avanca para o proximo no cuja condicao de transicao e satisfeita.
    /// Se nenhuma, permanece no no atual.
    pub fn advance(&mut self) -> usize {
        let current = self.current.unwrap_or(self.start_node);
        for edge in &self.edges {
            if edge.from == current && (edge.condition)() {
                self.current = Some(edge.to);
                return edge.to;
            }
        }
        current
    }

    /// Retorna o nome do agente que deve ser pollado agora
    pub fn current_agent(&self) -> Option<&str> {
        let idx = self.current.unwrap_or(self.start_node);
        if idx < self.nodes.len() {
            Some(&self.nodes[idx].agent_name)
        } else { None }
    }
}
