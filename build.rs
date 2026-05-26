//! build.rs — runs the schema-next macro engine (designer-assistant/375
//! feature branch) to lower spirit.schema with the imported
//! signal-frame.schema macro declarations resolved. The result is an
//! `Asschema` whose namespace contains BOTH the user-authored types
//! AND the macro-expanded `InputRoute` / `OutputRoute` / `ApexCodec`.
//! A local emitter then converts that asschema into Rust source.
//!
//! Per intent records 865-867 (designer-assistant/375).

use std::{env, fs, path::PathBuf};

use schema_next::{
    Asschema, EnumDeclaration, EnumVariant, ExpansionMacro, FieldDeclaration, MacroExpansion,
    MacroRegistry, MacroSignature, Name, RootSurface, SchemaEngine, SchemaError, SchemaIdentity,
    StructDeclaration, TypeDeclaration, TypeReference,
};

fn main() {
    println!("cargo:rerun-if-changed=schema/spirit.schema");
    println!("cargo:rerun-if-changed=schema/signal-frame.schema");
    println!("cargo:rerun-if-changed=build.rs");

    let schema_directory = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("manifest dir"))
        .join("schema");
    let spirit_path = schema_directory.join("spirit.schema");
    let source = fs::read_to_string(&spirit_path).expect("read spirit.schema");

    // Build a registry with the SignalFrame::Route + SignalFrame::SignalCodec
    // expansion macros pre-registered. The schema engine resolves the
    // `(Route Input)` / `(Route Output)` / `(SignalCodec Input Output)`
    // namespace entries against these.
    let mut registry = MacroRegistry::new();
    registry.register(SignalFrameRouteMacro);
    registry.register(SignalFrameSignalCodecMacro);

    let engine = SchemaEngine::with_registry(registry).with_base_directory(schema_directory);

    let asschema = engine
        .lower_source(&source, SchemaIdentity::new("design_deep_spirit_next_pass", "0.3.0"))
        .expect("lower spirit.schema");

    // Emit Rust source.
    let emitter = RustEmitter::new(&asschema);
    let code = emitter.emit();

    let output_directory = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR set"));
    fs::write(
        output_directory.join("spirit_generated.rs"),
        code,
    )
    .expect("write spirit_generated.rs");

    // Also emit the canonical asschema + hash for inspection.
    let canonical = asschema.to_canonical_nota();
    fs::write(
        output_directory.join("spirit.asschema.nota"),
        canonical,
    )
    .expect("write canonical asschema");
    let hash_hex = asschema.canonical_hash_hex();
    fs::write(
        output_directory.join("spirit.schema.hash"),
        &hash_hex,
    )
    .expect("write schema hash");
    // Expose the hash as a build-time env var so the daemon stamps
    // its redb meta table with this exact value (drives the schema-
    // version migration path).
    println!("cargo:rustc-env=DESIGN_DEEP_SPIRIT_NEXT_PASS_SCHEMA_HASH={hash_hex}");
}

// ====================================================================
// Expansion macros — registered with the schema engine. The
// `(Route Input)` namespace entry triggers this expansion.
// ====================================================================

/// Route macro — emits an enum `<CallSiteName>` with one unit variant per
/// surface variant. The call-site declaration name is the OUTPUT enum
/// name; the macro's INPUT argument is the SURFACE enum name to mirror.
struct SignalFrameRouteMacro;

impl ExpansionMacro for SignalFrameRouteMacro {
    fn qualified_name(&self) -> &'static str {
        "SignalFrame::Route"
    }

    fn signature(&self) -> MacroSignature {
        MacroSignature {
            input_shapes: vec![Name::new("SurfaceEnum")],
            output_shape: Name::new("RouteEnum"),
        }
    }

    fn expand(
        &self,
        call_site_name: &Name,
        arguments: &[Name],
        asschema_so_far: &Asschema,
    ) -> Result<MacroExpansion, SchemaError> {
        let argument = arguments
            .first()
            .ok_or_else(|| SchemaError::ExpectedSymbol {
                found: "Route macro requires a surface argument".to_owned(),
            })?;
        let surface = find_surface(asschema_so_far, argument).ok_or_else(|| {
            SchemaError::ExpectedSymbol {
                found: format!(
                    "Route macro could not find surface {}",
                    argument.as_str()
                ),
            }
        })?;
        let variants = surface
            .variants
            .iter()
            .map(|variant| EnumVariant {
                name: variant.name.clone(),
                payload: None,
            })
            .collect::<Vec<_>>();
        Ok(MacroExpansion::with_type(TypeDeclaration::Enum(
            EnumDeclaration {
                name: call_site_name.clone(),
                variants,
            },
        )))
    }
}

/// SignalCodec macro — emits a marker struct holding the input + output
/// surface references. The actual codec methods get emitted as `impl`
/// blocks on Input/Output by the RustEmitter based on the presence of
/// matching `<Surface>Route` enums in the namespace.
struct SignalFrameSignalCodecMacro;

impl ExpansionMacro for SignalFrameSignalCodecMacro {
    fn qualified_name(&self) -> &'static str {
        "SignalFrame::SignalCodec"
    }

    fn signature(&self) -> MacroSignature {
        MacroSignature {
            input_shapes: vec![Name::new("InputSurface"), Name::new("OutputSurface")],
            output_shape: Name::new("Codec"),
        }
    }

    fn expand(
        &self,
        call_site_name: &Name,
        arguments: &[Name],
        _asschema_so_far: &Asschema,
    ) -> Result<MacroExpansion, SchemaError> {
        let fields = arguments
            .iter()
            .map(|argument| FieldDeclaration {
                name: Name::new(argument.field_name()),
                reference: TypeReference {
                    name: argument.clone(),
                },
            })
            .collect();
        let codec = TypeDeclaration::Struct(StructDeclaration {
            name: call_site_name.clone(),
            fields,
        });
        Ok(MacroExpansion::with_type(codec))
    }
}

fn find_surface<'a>(asschema: &'a Asschema, name: &Name) -> Option<&'a RootSurface> {
    asschema
        .surfaces()
        .iter()
        .find(|surface| surface.name.as_str() == name.as_str())
}

// ====================================================================
// RustEmitter — converts a lowered asschema into Rust source. Built
// to consume the macro-expanded namespace: when both `Input` (surface)
// and `InputRoute` (enum, with matching variant names) are present,
// it emits short-header constants + the route+codec methods.
// ====================================================================

struct RustEmitter<'a> {
    asschema: &'a Asschema,
}

impl<'a> RustEmitter<'a> {
    fn new(asschema: &'a Asschema) -> Self {
        Self { asschema }
    }

    fn emit(&self) -> String {
        let mut writer = SourceWriter::default();
        writer.line("// @generated by design-deep-spirit-next-pass build.rs (designer-assistant/375)");
        writer.blank();
        writer.line("pub type Text = String;");
        writer.line("pub type Integer = u64;");
        writer.blank();
        self.emit_nota_runtime(&mut writer);
        writer.blank();

        // Emit all namespace types (includes macro-expanded ones).
        for declaration in self.asschema.namespace() {
            self.emit_type(&mut writer, declaration);
            writer.blank();
        }

        for surface in self.asschema.surfaces() {
            self.emit_surface(&mut writer, surface);
            writer.blank();
        }

        for declaration in self.asschema.namespace() {
            self.emit_nota_impl(&mut writer, declaration);
            writer.blank();
        }

        for surface in self.asschema.surfaces() {
            self.emit_nota_surface_impl(&mut writer, surface);
            writer.blank();
        }

        self.emit_signal_frame_support(&mut writer);
        writer.finish()
    }

    fn emit_nota_runtime(&self, writer: &mut SourceWriter) {
        writer.line("#[derive(Clone, Debug, PartialEq, Eq)]");
        writer.line("pub enum NotaDecodeError {");
        writer.line("    Parse(String),");
        writer.line("    ExpectedSingleRoot { found: usize },");
        writer.line("    ExpectedDelimited { type_name: &'static str, delimiter: &'static str },");
        writer.line("    ExpectedRootCount { type_name: &'static str, expected: usize, found: usize },");
        writer.line("    ExpectedAtleastRootCount { type_name: &'static str, expected: usize, found: usize },");
        writer.line("    ExpectedAtom { type_name: &'static str },");
        writer.line("    UnknownVariant { enum_name: &'static str, variant: String },");
        writer.line("    InvalidInteger { value: String },");
        writer.line("}");
        writer.blank();
        writer.line("impl std::fmt::Display for NotaDecodeError {");
        writer.line("    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {");
        writer.line("        write!(f, \"{self:?}\")");
        writer.line("    }");
        writer.line("}");
        writer.line("impl std::error::Error for NotaDecodeError {}");
        writer.blank();
        writer.line("pub struct NotaSupport;");
        writer.line("impl NotaSupport {");
        writer.line("    pub fn parse_root(source: &str) -> Result<nota_next::Block, NotaDecodeError> {");
        writer.line("        let document = nota_next::Document::parse(source).map_err(|e| NotaDecodeError::Parse(e.to_string()))?;");
        writer.line("        if document.holds_root_objects() != 1 {");
        writer.line("            return Err(NotaDecodeError::ExpectedSingleRoot { found: document.holds_root_objects() });");
        writer.line("        }");
        writer.line("        Ok(document.root_object_at(0).expect(\"checked\").clone())");
        writer.line("    }");
        writer.blank();
        writer.line("    pub fn expect_children<'a>(block: &'a nota_next::Block, delimiter: nota_next::Delimiter, delimiter_name: &'static str, type_name: &'static str, expected: usize) -> Result<&'a [nota_next::Block], NotaDecodeError> {");
        writer.line("        match block {");
        writer.line("            nota_next::Block::Delimited { delimiter: found, root_objects, .. } if *found == delimiter => {");
        writer.line("                if root_objects.len() != expected {");
        writer.line("                    return Err(NotaDecodeError::ExpectedRootCount { type_name, expected, found: root_objects.len() });");
        writer.line("                }");
        writer.line("                Ok(root_objects)");
        writer.line("            }");
        writer.line("            _ => Err(NotaDecodeError::ExpectedDelimited { type_name, delimiter: delimiter_name }),");
        writer.line("        }");
        writer.line("    }");
        writer.blank();
        writer.line("    pub fn expect_children_at_least<'a>(block: &'a nota_next::Block, delimiter: nota_next::Delimiter, delimiter_name: &'static str, type_name: &'static str, expected: usize) -> Result<&'a [nota_next::Block], NotaDecodeError> {");
        writer.line("        match block {");
        writer.line("            nota_next::Block::Delimited { delimiter: found, root_objects, .. } if *found == delimiter => {");
        writer.line("                if root_objects.len() < expected {");
        writer.line("                    return Err(NotaDecodeError::ExpectedAtleastRootCount { type_name, expected, found: root_objects.len() });");
        writer.line("                }");
        writer.line("                Ok(root_objects)");
        writer.line("            }");
        writer.line("            _ => Err(NotaDecodeError::ExpectedDelimited { type_name, delimiter: delimiter_name }),");
        writer.line("        }");
        writer.line("    }");
        writer.blank();
        writer.line("    pub fn parse_text(block: &nota_next::Block) -> Result<Text, NotaDecodeError> {");
        writer.line("        if let Some(text) = block.demote_to_string() { return Ok(text.to_owned()); }");
        writer.line("        match block {");
        writer.line("            nota_next::Block::Delimited { delimiter: nota_next::Delimiter::SquareBracket, root_objects, .. } => {");
        writer.line("                root_objects.iter().map(Self::parse_text).collect::<Result<Vec<_>, _>>().map(|parts| parts.join(\" \"))");
        writer.line("            }");
        writer.line("            _ => Err(NotaDecodeError::ExpectedDelimited { type_name: \"Text\", delimiter: \"text atom or square bracket\" }),");
        writer.line("        }");
        writer.line("    }");
        writer.blank();
        writer.line("    pub fn format_text(value: &str) -> String {");
        writer.line("        if value.contains(\"|]\") { format!(\"[{}]\", value.replace(']', \" ]\")) }");
        writer.line("        else if value.chars().any(|c| matches!(c, '[' | ']' | '(' | ')' | '{' | '}' | ';' | '\\n')) { format!(\"[|{value}|]\") }");
        writer.line("        else { format!(\"[{value}]\") }");
        writer.line("    }");
        writer.blank();
        writer.line("    pub fn parse_integer(block: &nota_next::Block) -> Result<Integer, NotaDecodeError> {");
        writer.line("        let value = block.demote_to_string().ok_or(NotaDecodeError::ExpectedAtom { type_name: \"Integer\" })?;");
        writer.line("        value.parse::<Integer>().map_err(|_| NotaDecodeError::InvalidInteger { value: value.to_owned() })");
        writer.line("    }");
        writer.line("}");
    }

    fn emit_type(&self, writer: &mut SourceWriter, declaration: &TypeDeclaration) {
        match declaration {
            TypeDeclaration::Struct(struct_decl) => self.emit_struct(writer, struct_decl),
            TypeDeclaration::Newtype(struct_decl) => self.emit_newtype(writer, struct_decl),
            TypeDeclaration::Enum(enum_decl) => self.emit_enum(writer, enum_decl),
        }
    }

    fn emit_newtype(&self, writer: &mut SourceWriter, struct_decl: &StructDeclaration) {
        let field = struct_decl
            .fields
            .first()
            .expect("newtype has one field");
        writer.line("#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize, Clone, Debug, PartialEq, Eq)]");
        writer.line(format!(
            "pub struct {}(pub {});",
            struct_decl.name,
            rust_type(&field.reference)
        ));
    }

    fn emit_struct(&self, writer: &mut SourceWriter, struct_decl: &StructDeclaration) {
        writer.line("#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize, Clone, Debug, PartialEq, Eq)]");
        writer.line(format!("pub struct {} {{", struct_decl.name));
        for field in &struct_decl.fields {
            writer.line(format!(
                "    pub {}: {},",
                field.name.as_str(),
                rust_type(&field.reference)
            ));
        }
        writer.line("}");
    }

    fn emit_enum(&self, writer: &mut SourceWriter, enum_decl: &EnumDeclaration) {
        writer.line("#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize, Clone, Debug, PartialEq, Eq)]");
        writer.line(format!("pub enum {} {{", enum_decl.name));
        for variant in &enum_decl.variants {
            match &variant.payload {
                Some(reference) => writer.line(format!(
                    "    {}({}),",
                    variant.name,
                    rust_type(reference)
                )),
                None => writer.line(format!("    {},", variant.name)),
            }
        }
        writer.line("}");
    }

    fn emit_surface(&self, writer: &mut SourceWriter, surface: &RootSurface) {
        writer.line("#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize, Clone, Debug, PartialEq, Eq)]");
        writer.line(format!("pub enum {} {{", surface.name));
        for variant in &surface.variants {
            match &variant.payload {
                Some(reference) => writer.line(format!(
                    "    {}({}),",
                    variant.name,
                    rust_type(reference)
                )),
                None => writer.line(format!("    {},", variant.name)),
            }
        }
        writer.line("}");
    }

    fn emit_nota_impl(&self, writer: &mut SourceWriter, declaration: &TypeDeclaration) {
        match declaration {
            TypeDeclaration::Struct(struct_decl) => self.emit_nota_struct_impl(writer, struct_decl),
            TypeDeclaration::Newtype(struct_decl) => self.emit_nota_newtype_impl(writer, struct_decl),
            TypeDeclaration::Enum(enum_decl) => {
                self.emit_nota_enum_impl(writer, &enum_decl.name, &enum_decl.variants)
            }
        }
    }

    fn emit_nota_newtype_impl(&self, writer: &mut SourceWriter, struct_decl: &StructDeclaration) {
        let field = struct_decl.fields.first().expect("newtype has one field");
        writer.line(format!("impl {} {{", struct_decl.name));
        writer.line("    pub fn from_nota_block(block: &nota_next::Block) -> Result<Self, NotaDecodeError> {");
        writer.line(format!(
            "        Ok(Self({}))",
            parse_expression(&field.reference, "block")
        ));
        writer.line("    }");
        writer.blank();
        writer.line("    pub fn to_nota(&self) -> String {");
        writer.line(format!(
            "        {}",
            format_expression(&field.reference, "self.0")
        ));
        writer.line("    }");
        writer.line("}");
    }

    fn emit_nota_struct_impl(&self, writer: &mut SourceWriter, struct_decl: &StructDeclaration) {
        writer.line(format!("impl {} {{", struct_decl.name));
        writer.line("    pub fn from_nota_block(block: &nota_next::Block) -> Result<Self, NotaDecodeError> {");
        writer.line(format!(
            "        let children = NotaSupport::expect_children(block, nota_next::Delimiter::Parenthesis, \"parenthesis\", \"{}\", {})?;",
            struct_decl.name,
            struct_decl.fields.len()
        ));
        writer.line("        Ok(Self {");
        for (index, field) in struct_decl.fields.iter().enumerate() {
            writer.line(format!(
                "            {}: {},",
                field.name.as_str(),
                parse_expression(&field.reference, &format!("&children[{index}]"))
            ));
        }
        writer.line("        })");
        writer.line("    }");
        writer.blank();
        writer.line("    pub fn to_nota(&self) -> String {");
        writer.line("        let fields = [");
        for field in &struct_decl.fields {
            writer.line(format!(
                "            {},",
                format_expression(&field.reference, &format!("self.{}", field.name.as_str()))
            ));
        }
        writer.line("        ];");
        writer.line("        format!(\"({})\", fields.join(\" \"))");
        writer.line("    }");
        writer.line("}");
    }

    fn emit_nota_enum_impl(
        &self,
        writer: &mut SourceWriter,
        name: &Name,
        variants: &[EnumVariant],
    ) {
        let unit_variants: Vec<&EnumVariant> = variants
            .iter()
            .filter(|variant| variant.payload.is_none())
            .collect();
        let payload_variants: Vec<&EnumVariant> = variants
            .iter()
            .filter(|variant| variant.payload.is_some())
            .collect();
        writer.line(format!("impl {name} {{"));
        writer.line("    pub fn from_nota_block(block: &nota_next::Block) -> Result<Self, NotaDecodeError> {");
        writer.line("        if let Some(variant) = block.demote_to_string() {");
        if unit_variants.is_empty() {
            writer.line(format!(
                "            return Err(NotaDecodeError::UnknownVariant {{ enum_name: \"{name}\", variant: variant.to_owned() }});"
            ));
        } else {
            writer.line("            return match variant {");
            for variant in &unit_variants {
                writer.line(format!(
                    "                \"{}\" => Ok(Self::{}),",
                    variant.name, variant.name
                ));
            }
            writer.line(format!(
                "                other => Err(NotaDecodeError::UnknownVariant {{ enum_name: \"{name}\", variant: other.to_owned() }}),"
            ));
            writer.line("            };");
        }
        writer.line("        }");
        if payload_variants.is_empty() {
            writer.line(format!(
                "        Err(NotaDecodeError::ExpectedAtom {{ type_name: \"{name}\" }})"
            ));
            writer.line("    }");
            writer.blank();
            self.emit_nota_enum_formatter(writer, variants);
            writer.line("}");
            return;
        }
        writer.line(format!(
            "        let children = NotaSupport::expect_children(block, nota_next::Delimiter::Parenthesis, \"parenthesis\", \"{name}\", 2)?;"
        ));
        writer.line("        let variant = children[0].demote_to_string().ok_or(NotaDecodeError::ExpectedAtom { type_name: \"enum variant\" })?;");
        writer.line("        match variant {");
        for variant in &payload_variants {
            let payload = variant.payload.as_ref().expect("filtered payload");
            writer.line(format!(
                "            \"{}\" => Ok(Self::{}({})),",
                variant.name,
                variant.name,
                parse_expression(payload, "&children[1]")
            ));
        }
        writer.line(format!(
            "            other => Err(NotaDecodeError::UnknownVariant {{ enum_name: \"{name}\", variant: other.to_owned() }}),"
        ));
        writer.line("        }");
        writer.line("    }");
        writer.blank();
        self.emit_nota_enum_formatter(writer, variants);
        writer.line("}");
    }

    fn emit_nota_enum_formatter(&self, writer: &mut SourceWriter, variants: &[EnumVariant]) {
        writer.line("    pub fn to_nota(&self) -> String {");
        writer.line("        match self {");
        for variant in variants {
            match &variant.payload {
                Some(payload) => writer.line(format!(
                    "            Self::{}(payload) => format!(\"({} {{}})\", {}),",
                    variant.name,
                    variant.name,
                    format_expression(payload, "payload")
                )),
                None => writer.line(format!(
                    "            Self::{} => \"{}\".to_owned(),",
                    variant.name, variant.name
                )),
            }
        }
        writer.line("        }");
        writer.line("    }");
    }

    fn emit_nota_surface_impl(&self, writer: &mut SourceWriter, surface: &RootSurface) {
        self.emit_nota_enum_impl(writer, &surface.name, &surface.variants);
        writer.blank();
        writer.line(format!("impl std::str::FromStr for {} {{", surface.name));
        writer.line("    type Err = NotaDecodeError;");
        writer.blank();
        writer.line("    fn from_str(source: &str) -> Result<Self, Self::Err> {");
        writer.line("        let root = NotaSupport::parse_root(source)?;");
        writer.line("        Self::from_nota_block(&root)");
        writer.line("    }");
        writer.line("}");
        writer.blank();
        writer.line(format!("impl std::fmt::Display for {} {{", surface.name));
        writer.line("    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {");
        writer.line("        f.write_str(&self.to_nota())");
        writer.line("    }");
        writer.line("}");
    }

    /// Signal-frame support — emits SignalFrameError + short-header
    /// constants + route/codec methods. The Route enums themselves
    /// were already emitted as ordinary enums above (they came from
    /// macro expansion). This step adds the impl blocks that bridge
    /// surfaces to their route enum.
    fn emit_signal_frame_support(&self, writer: &mut SourceWriter) {
        writer.line("// ------------------------------------------------------------------");
        writer.line("// Signal-frame support — bridges schema-emitted Route enums to the");
        writer.line("// surface enums via short-header constants. Per signal-frame.schema");
        writer.line("// (macro-expanded by schema-next designer-finish-macro-engine branch).");
        writer.line("// ------------------------------------------------------------------");
        writer.line("const SIGNAL_SHORT_HEADER_BYTE_COUNT: usize = 8;");
        writer.blank();
        writer.line("#[derive(Clone, Debug, PartialEq, Eq)]");
        writer.line("pub enum SignalFrameError {");
        writer.line("    ArchiveEncode,");
        writer.line("    ArchiveDecode,");
        writer.line("    FrameTooShort { found: usize },");
        writer.line("    UnknownHeader { surface: &'static str, header: u64 },");
        writer.line("    HeaderMismatch { expected: u64, found: u64 },");
        writer.line("}");
        writer.blank();
        writer.line("impl std::fmt::Display for SignalFrameError {");
        writer.line("    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {");
        writer.line("        write!(f, \"{self:?}\")");
        writer.line("    }");
        writer.line("}");
        writer.line("impl std::error::Error for SignalFrameError {}");
        writer.blank();

        // Find each `<Surface>Route` enum in the namespace + its
        // corresponding surface, then emit constants + impl on the
        // surface.
        let surfaces = self.asschema.surfaces();
        writer.line("pub mod short_header {");
        for (surface_index, surface) in surfaces.iter().enumerate() {
            for (variant_index, variant) in surface.variants.iter().enumerate() {
                let constant = format!(
                    "{}_{}",
                    constant_name(&surface.name),
                    constant_name(&variant.name)
                );
                let value = ((surface_index as u64) << 56) | ((variant_index as u64) << 48);
                writer.line(format!(
                    "    pub const {constant}: u64 = 0x{value:016X};"
                ));
            }
        }
        writer.line("}");
        writer.blank();

        for surface in surfaces {
            let route_name = format!("{}Route", surface.name);
            // Verify the route enum exists in the namespace (came from
            // macro expansion). If it doesn't, surfaces emit without
            // route methods.
            if !route_enum_present(self.asschema, &route_name) {
                continue;
            }
            self.emit_signal_frame_impl(writer, surface, &route_name);
            writer.blank();
        }
    }

    fn emit_signal_frame_impl(
        &self,
        writer: &mut SourceWriter,
        surface: &RootSurface,
        route_name: &str,
    ) {
        writer.line(format!("impl {} {{", surface.name));
        writer.line(format!("    pub fn route(&self) -> {route_name} {{"));
        writer.line("        match self {");
        for variant in &surface.variants {
            match &variant.payload {
                Some(_) => writer.line(format!(
                    "            Self::{}(_) => {route_name}::{},",
                    variant.name, variant.name
                )),
                None => writer.line(format!(
                    "            Self::{} => {route_name}::{},",
                    variant.name, variant.name
                )),
            }
        }
        writer.line("        }");
        writer.line("    }");
        writer.blank();
        writer.line("    pub fn short_header(&self) -> u64 {");
        writer.line("        match self {");
        for variant in &surface.variants {
            let constant = format!(
                "{}_{}",
                constant_name(&surface.name),
                constant_name(&variant.name)
            );
            match &variant.payload {
                Some(_) => writer.line(format!(
                    "            Self::{}(_) => short_header::{constant},",
                    variant.name
                )),
                None => writer.line(format!(
                    "            Self::{} => short_header::{constant},",
                    variant.name
                )),
            }
        }
        writer.line("        }");
        writer.line("    }");
        writer.blank();
        writer.line(format!(
            "    pub fn route_from_short_header(header: u64) -> Result<{route_name}, SignalFrameError> {{"
        ));
        writer.line("        match header {");
        for variant in &surface.variants {
            let constant = format!(
                "{}_{}",
                constant_name(&surface.name),
                constant_name(&variant.name)
            );
            writer.line(format!(
                "            short_header::{constant} => Ok({route_name}::{}),",
                variant.name
            ));
        }
        writer.line(format!(
            "            _ => Err(SignalFrameError::UnknownHeader {{ surface: \"{}\", header }}),",
            surface.name
        ));
        writer.line("        }");
        writer.line("    }");
        writer.blank();
        writer.line("    pub fn encode_signal_frame(&self) -> Result<Vec<u8>, SignalFrameError> {");
        writer.line("        let archive = rkyv::to_bytes::<rkyv::rancor::Error>(self).map_err(|_| SignalFrameError::ArchiveEncode)?;");
        writer.line("        let mut frame = Vec::with_capacity(SIGNAL_SHORT_HEADER_BYTE_COUNT + archive.len());");
        writer.line("        frame.extend_from_slice(&self.short_header().to_le_bytes());");
        writer.line("        frame.extend_from_slice(&archive);");
        writer.line("        Ok(frame)");
        writer.line("    }");
        writer.blank();
        writer.line(format!(
            "    pub fn decode_signal_frame(frame: &[u8]) -> Result<({route_name}, Self), SignalFrameError> {{"
        ));
        writer.line("        if frame.len() < SIGNAL_SHORT_HEADER_BYTE_COUNT { return Err(SignalFrameError::FrameTooShort { found: frame.len() }); }");
        writer.line("        let mut header_bytes = [0_u8; SIGNAL_SHORT_HEADER_BYTE_COUNT];");
        writer.line("        header_bytes.copy_from_slice(&frame[..SIGNAL_SHORT_HEADER_BYTE_COUNT]);");
        writer.line("        let header = u64::from_le_bytes(header_bytes);");
        writer.line("        let route = Self::route_from_short_header(header)?;");
        writer.line("        let value = rkyv::from_bytes::<Self, rkyv::rancor::Error>(&frame[SIGNAL_SHORT_HEADER_BYTE_COUNT..]).map_err(|_| SignalFrameError::ArchiveDecode)?;");
        writer.line("        let expected = value.short_header();");
        writer.line("        if expected != header { return Err(SignalFrameError::HeaderMismatch { expected, found: header }); }");
        writer.line("        Ok((route, value))");
        writer.line("    }");
        writer.line("}");
    }
}

fn route_enum_present(asschema: &Asschema, route_name: &str) -> bool {
    asschema
        .type_named(route_name)
        .is_some_and(|declaration| matches!(declaration, TypeDeclaration::Enum(_)))
}

#[derive(Default)]
struct SourceWriter {
    output: String,
}

impl SourceWriter {
    fn line(&mut self, line: impl AsRef<str>) {
        self.output.push_str(line.as_ref());
        self.output.push('\n');
    }

    fn blank(&mut self) {
        self.output.push('\n');
    }

    fn finish(self) -> String {
        self.output
    }
}

fn parse_expression(reference: &TypeReference, block: &str) -> String {
    match reference.name.as_str() {
        "Text" => format!("NotaSupport::parse_text({block})?"),
        "Integer" => format!("NotaSupport::parse_integer({block})?"),
        name => format!("{name}::from_nota_block({block})?"),
    }
}

fn format_expression(reference: &TypeReference, value: &str) -> String {
    match reference.name.as_str() {
        "Text" => format!("NotaSupport::format_text(&{value})"),
        "Integer" => format!("{value}.to_string()"),
        _ => format!("{value}.to_nota()"),
    }
}

fn rust_type(reference: &TypeReference) -> String {
    match reference.name.as_str() {
        "Text" => "Text".to_owned(),
        "Integer" => "Integer".to_owned(),
        name => name.to_owned(),
    }
}

fn constant_name(name: &Name) -> String {
    let mut output = String::new();
    for (index, character) in name.as_str().chars().enumerate() {
        if character.is_ascii_uppercase() {
            if index > 0 {
                output.push('_');
            }
            output.push(character);
        } else if character == '-' {
            output.push('_');
        } else {
            output.push(character.to_ascii_uppercase());
        }
    }
    output
}
