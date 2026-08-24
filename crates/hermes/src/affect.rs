#[derive(Debug, Clone, Copy)]
pub struct AffectVector {
    pub valence: f32,
    pub arousal: f32,
    pub dominance: f32,
    pub uncertainty: f32,
    pub urgency: f32,
    pub fatigue: f32,
    pub curiosity: f32,
    pub coherence: f32,
}

impl AffectVector {
    pub fn neutral() -> Self {
        AffectVector {
            valence: 0.0, arousal: 0.5, dominance: 0.5,
            uncertainty: 0.0, urgency: 0.0, fatigue: 0.0,
            curiosity: 0.3, coherence: 0.8,
        }
    }

    pub fn valence_to_rgb(&self) -> (u8, u8, u8) {
        if self.valence >= 0.0 {
            let g = 128 + (self.valence * 127.0) as u8;
            (255 - g, 255, 255 - g)
        } else {
            let r = 128 + (-self.valence * 127.0) as u8;
            (255, 255 - r, 255 - r)
        }
    }

    pub fn arousal_to_pulse(&self) -> f32 {
        0.5 + self.arousal * 0.5
    }

    pub fn dominance_to_size(&self) -> f32 {
        0.5 + self.dominance * 0.5
    }

    pub fn curiosity_to_rings(&self) -> u32 {
        (self.curiosity * 5.0) as u32
    }
}

#[derive(Debug, Clone, Copy)]
pub enum AffectEvent {
    Success(f32),
    Error(f32),
    Timeout,
    Novelty(f32),
    UserSatisfaction(f32),
}


pub struct AffectRegulator {
    pub affect: AffectVector,
    decay_rate: f32,
}

impl AffectRegulator {
    pub fn new() -> Self {
        AffectRegulator { affect: AffectVector::neutral(), decay_rate: 0.05 }
    }

    pub fn incorporate(&mut self, event: AffectEvent) {
        match event {
            AffectEvent::Success(confidence) => {
                self.affect.valence += 0.1 * confidence;
                self.affect.coherence += 0.05;
                self.affect.fatigue += 0.02;
                self.affect.uncertainty *= 0.9;
            }
            AffectEvent::Error(severity) => {
                self.affect.valence -= 0.2 * severity;
                self.affect.uncertainty += 0.2 * severity;
                self.affect.fatigue += 0.1;
                self.affect.coherence -= 0.1;
            }
            AffectEvent::Timeout => {
                self.affect.urgency += 0.2;
                self.affect.fatigue += 0.15;
                self.affect.valence -= 0.05;
            }
            AffectEvent::Novelty(magnitude) => {
                self.affect.curiosity += 0.3 * magnitude;
                self.affect.arousal += 0.2 * magnitude;
                self.affect.uncertainty += 0.1 * magnitude;
            }
            AffectEvent::UserSatisfaction(level) => {
                self.affect.valence += 0.2 * level;
                self.affect.coherence += 0.1 * level;
                self.affect.urgency *= 0.8;
            }
        }
        self.clamp();
    }

    pub fn decay(&mut self) {
        let neutral = AffectVector::neutral();
        self.affect.valence += (neutral.valence - self.affect.valence) * self.decay_rate;
        self.affect.arousal += (neutral.arousal - self.affect.arousal) * self.decay_rate;
        self.affect.dominance += (neutral.dominance - self.affect.dominance) * self.decay_rate;
        self.affect.uncertainty += (neutral.uncertainty - self.affect.uncertainty) * self.decay_rate;
        self.affect.urgency += (neutral.urgency - self.affect.urgency) * self.decay_rate;
        self.affect.fatigue += (neutral.fatigue - self.affect.fatigue) * self.decay_rate;
        self.affect.curiosity += (neutral.curiosity - self.affect.curiosity) * self.decay_rate;
        self.affect.coherence += (neutral.coherence - self.affect.coherence) * self.decay_rate;
    }

    pub fn affect_modulated_score(&self, raw_score: f32) -> f32 {
        let mut score = raw_score;
        if self.affect.urgency > 0.7 { score *= 1.5; }
        if self.affect.uncertainty > 0.6 { score *= 0.7; }
        if self.affect.fatigue > 0.8 { score *= 0.5; }
        if self.affect.curiosity > 0.7 { score *= 1.3; }
        if self.affect.coherence < 0.3 { score *= 0.3; }
        score
    }

    fn clamp(&mut self) {
        self.affect.valence = self.affect.valence.clamp(-1.0, 1.0);
        self.affect.arousal = self.affect.arousal.clamp(0.0, 1.0);
        self.affect.dominance = self.affect.dominance.clamp(0.0, 1.0);
        self.affect.uncertainty = self.affect.uncertainty.clamp(0.0, 1.0);
        self.affect.urgency = self.affect.urgency.clamp(0.0, 1.0);
        self.affect.fatigue = self.affect.fatigue.clamp(0.0, 1.0);
        self.affect.curiosity = self.affect.curiosity.clamp(0.0, 1.0);
        self.affect.coherence = self.affect.coherence.clamp(0.0, 1.0);
    }
}

pub struct SoulMirrorUpdate {
    pub color: (u8, u8, u8),
    pub pulse: f32,
    pub size: f32,
    pub rings: u32,
    pub rotation: u32,
}

pub struct SoulMirror;

impl SoulMirror {
    pub fn from_affect(affect: &AffectVector) -> SoulMirrorUpdate {
        SoulMirrorUpdate {
            color: affect.valence_to_rgb(),
            pulse: affect.arousal_to_pulse(),
            size: affect.dominance_to_size(),
            rings: affect.curiosity_to_rings(),
            rotation: (affect.urgency * 360.0) as u32,
        }
    }
}






