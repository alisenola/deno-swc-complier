use std::error::Error;
use std::fmt;
use std::sync::Arc;
use std::sync::RwLock;
use swc_common::errors::Emitter;
use swc_common::{
    self,
    comments::Comments,
    errors::{Diagnostic, DiagnosticBuilder, Handler, HandlerFlags},
    FileName, Globals, SourceMap,
};
use swc_ecma_parser::{
    lexer::Lexer, JscTarget, Parser, Session, SourceFileInput, Syntax, TsConfig,
};

#[derive(Clone, Debug)]
pub struct SwcDiagnosticBuffer {
    pub diagnostics: Vec<Diagnostic>,
}

impl Error for SwcDiagnosticBuffer {}

impl fmt::Display for SwcDiagnosticBuffer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = self
            .diagnostics
            .iter()
            .map(|d| d.message())
            .collect::<Vec<String>>()
            .join(",");

        f.pad(&msg)
    }
}

#[derive(Clone)]
pub struct SwcErrorBuffer(Arc<RwLock<SwcDiagnosticBuffer>>);

impl SwcErrorBuffer {
    pub fn default() -> Self {
        Self(Arc::new(RwLock::new(SwcDiagnosticBuffer {
            diagnostics: vec![],
        })))
    }
}

impl Emitter for SwcErrorBuffer {
    fn emit(&mut self, db: &DiagnosticBuilder) {
        self.0.write().unwrap().diagnostics.push((**db).clone());
    }
}

impl From<SwcErrorBuffer> for SwcDiagnosticBuffer {
    fn from(buf: SwcErrorBuffer) -> Self {
        let s = buf.0.read().unwrap();
        s.clone()
    }
}

/// Low-level utility structure with common AST parsing functions.
///
/// Allows to build more complicated parser by providing a callback
/// to `parse_module`.
pub struct AstParser {
    pub buffered_error: SwcErrorBuffer,
    pub source_map: Arc<SourceMap>,
    pub handler: Handler,
    pub comments: Comments,
    pub globals: Globals,
}

impl AstParser {
    pub fn new() -> Self {
        let buffered_error = SwcErrorBuffer::default();

        let handler = Handler::with_emitter_and_flags(
            Box::new(buffered_error.clone()),
            HandlerFlags {
                dont_buffer_diagnostics: true,
                can_emit_warnings: true,
                ..Default::default()
            },
        );

        AstParser {
            buffered_error,
            source_map: Arc::new(SourceMap::default()),
            handler,
            comments: Comments::default(),
            globals: Globals::new(),
        }
    }

    pub fn parse_module<F, R>(&self, file_name: &str, source_code: &str, callback: F) -> R
    where
        F: FnOnce(Result<swc_ecma_ast::Module, SwcDiagnosticBuffer>) -> R,
    {
        swc_common::GLOBALS.set(&self.globals, || {
            let swc_source_file = self.source_map.new_source_file(
                FileName::Custom(file_name.to_string()),
                source_code.to_string(),
            );

            let buffered_err = self.buffered_error.clone();
            let session = Session {
                handler: &self.handler,
            };

            let mut ts_config = TsConfig::default();
            ts_config.dynamic_import = true;
            let syntax = Syntax::Typescript(ts_config);

            let lexer = Lexer::new(
                session,
                syntax,
                JscTarget::Es2019,
                SourceFileInput::from(&*swc_source_file),
                Some(&self.comments),
            );

            let mut parser = Parser::new_from(session, lexer);

            let parse_result = parser
                .parse_module()
                .map_err(move |mut err: DiagnosticBuilder| {
                    err.cancel();
                    SwcDiagnosticBuffer::from(buffered_err)
                });

            callback(parse_result)
        })
    }
}

impl Default for AstParser {
    fn default() -> Self {
        Self::new()
    }
}
