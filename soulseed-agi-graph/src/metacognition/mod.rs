//! 元认知工具集 (STL - Self-Thinking Library)
//!
//! 依据文档:
//! - 6-元认知工具/0-元认知工具-1/01-元认知工具-AI-01-谷歌gemini.md
//! - 6-元认知工具/0-元认知工具-1/02-SurrealDB原生功能的革命性集成.md
//! - 6-元认知工具/1-元认知工具-2/01-对话事件类型以及元认知工具.md
//!
//! 本模块实现了四大元认知工具:
//! - CausalChainTracer: 因果链追踪器
//! - PerformanceProfiler: 性能剖析器
//! - DecisionAuditor: 决策审计器
//! - PatternDetector: 模式检测器
//!
//! 以及统一入口:
//! - UnifiedMetacognitiveAnalyzer: 统一元认知分析器

pub mod analyzer;
pub mod auditor;
pub mod causal_tracer;
pub mod pattern_detector;
pub mod profiler;
pub mod types;

pub use analyzer::*;
pub use auditor::*;
pub use causal_tracer::*;
pub use pattern_detector::*;
pub use profiler::*;
pub use types::*;
