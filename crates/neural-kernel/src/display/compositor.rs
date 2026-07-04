//! Console Hermes — chat simples (NousResearch-style).
//! Substitui o compositor multi-window bugado.

use spin::Mutex;
use crate::display::console::NeuralConsole;

/// Static global do Hermes Chat Console. Inicializado pelo DisplayAgent.
pub static COMPOSITOR: Mutex<Option<NeuralConsole>> = Mutex::new(None);
