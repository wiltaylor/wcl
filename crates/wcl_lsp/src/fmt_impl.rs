// Minimal WCL formatter for LSP formatting support.
// Mirrors wcl_cli/src/fmt.rs logic.

use wcl_core::ast::*;

pub fn format_document(doc: &Document) -> String {
    let mut output = String::new();
    let mut f = Fmt {
        out: &mut output,
        indent: 0,
    };
    f.doc(doc);
    output
}

struct Fmt<'a> {
    out: &'a mut String,
    indent: usize,
}

impl<'a> Fmt<'a> {
    fn indent(&mut self) {
        for _ in 0..self.indent {
            self.out.push_str("    ");
        }
    }

    fn doc(&mut self, doc: &Document) {
        for (i, item) in doc.items.iter().enumerate() {
            if i > 0 {
                self.out.push('\n');
            }
            self.doc_item(item);
        }
        if !doc.items.is_empty() {
            self.out.push('\n');
        }
    }

    fn doc_item(&mut self, item: &DocItem) {
        match item {
            DocItem::Import(import) => {
                self.indent();
                self.out.push_str("import ");
                if import.kind == ImportKind::Library {
                    self.out.push('<');
                    for part in &import.path.parts {
                        if let StringPart::Literal(s) = part {
                            self.out.push_str(s);
                        }
                    }
                    self.out.push('>');
                } else {
                    self.string_lit(&import.path);
                }
                self.out.push('\n');
            }
            DocItem::ExportLet(el) => {
                self.indent();
                self.out
                    .push_str(&format!("export let {} = ", el.name.name));
                self.expr(&el.value);
                self.out.push('\n');
            }
            DocItem::ReExport(re) => {
                self.indent();
                self.out.push_str(&format!("export {}\n", re.name.name));
            }
            DocItem::Body(body_item) => self.body_item(body_item),
            DocItem::FunctionDecl(decl) => {
                self.indent();
                self.out.push_str(&format!("declare {}(", decl.name.name));
                for (i, param) in decl.params.iter().enumerate() {
                    if i > 0 {
                        self.out.push_str(", ");
                    }
                    self.out.push_str(&format!("{}: ", param.name.name));
                    self.type_expr(&param.type_expr);
                }
                self.out.push(')');
                if let Some(ref rt) = decl.return_type {
                    self.out.push_str(" -> ");
                    self.type_expr(rt);
                }
                self.out.push('\n');
            }
        }
    }

    fn body_item(&mut self, item: &BodyItem) {
        match item {
            BodyItem::Attribute(attr) => {
                for dec in &attr.decorators {
                    self.decorator(dec);
                }
                self.indent();
                self.out.push_str(&format!("{} = ", attr.name.name));
                self.expr(&attr.value);
                self.out.push('\n');
            }
            BodyItem::Block(block) => {
                for dec in &block.decorators {
                    self.decorator(dec);
                }
                self.indent();
                if block.partial {
                    self.out.push_str("partial ");
                }
                self.out.push_str(&block.kind.name);
                if let Some(id) = &block.inline_id {
                    self.out.push(' ');
                    match id {
                        InlineId::Literal(lit) => self.out.push_str(&lit.value),
                        InlineId::Interpolated(parts) => {
                            for part in parts {
                                match part {
                                    StringPart::Literal(s) => self.out.push_str(s),
                                    StringPart::Interpolation(e) => {
                                        self.out.push_str("${");
                                        self.expr(e);
                                        self.out.push('}');
                                    }
                                }
                            }
                        }
                    }
                }
                for arg in &block.inline_args {
                    self.out.push(' ');
                    self.expr(arg);
                }
                if let Some(ref tc) = block.text_content {
                    self.out.push(' ');
                    self.string_lit(tc);
                    self.out.push('\n');
                } else {
                    self.out.push_str(" {\n");
                    self.indent += 1;
                    for child in &block.body {
                        self.body_item(child);
                    }
                    self.indent -= 1;
                    self.indent();
                    self.out.push_str("}\n");
                }
            }
            BodyItem::LetBinding(lb) => {
                self.indent();
                self.out.push_str(&format!("let {} = ", lb.name.name));
                self.expr(&lb.value);
                self.out.push('\n');
            }
            BodyItem::Table(table) => {
                self.indent();
                if table.partial {
                    self.out.push_str("partial ");
                }
                self.out.push_str("table");
                if let Some(id) = &table.inline_id {
                    self.out.push(' ');
                    match id {
                        InlineId::Literal(lit) => self.out.push_str(&lit.value),
                        InlineId::Interpolated(_) => self.out.push_str("<interpolated>"),
                    }
                }
                if let Some(ref sr) = table.schema_ref {
                    self.out.push_str(&format!(" : {}", sr.name));
                }
                if let Some(ref import) = table.import_expr {
                    self.out.push_str(" = ");
                    self.expr(import);
                    self.out.push('\n');
                } else {
                    self.out.push_str(" {\n");
                    self.indent += 1;
                    for col in &table.columns {
                        self.indent();
                        self.out.push_str(&format!("{} : ", col.name.name));
                        self.type_expr(&col.type_expr);
                        self.out.push('\n');
                    }
                    if !table.columns.is_empty() && !table.rows.is_empty() {
                        self.out.push('\n');
                    }
                    for row in &table.rows {
                        self.indent();
                        for cell in &row.cells {
                            self.out.push_str("| ");
                            self.expr(cell);
                            self.out.push(' ');
                        }
                        self.out.push_str("|\n");
                    }
                    self.indent -= 1;
                    self.indent();
                    self.out.push_str("}\n");
                }
            }
            BodyItem::ForLoop(fl) => {
                self.indent();
                self.out.push_str(&format!("for {}", fl.iterator.name));
                if let Some(idx) = &fl.index {
                    self.out.push_str(&format!(", {}", idx.name));
                }
                self.out.push_str(" in ");
                self.expr(&fl.iterable);
                self.out.push_str(" {\n");
                self.indent += 1;
                for child in &fl.body {
                    self.body_item(child);
                }
                self.indent -= 1;
                self.indent();
                self.out.push_str("}\n");
            }
            BodyItem::Conditional(cond) => {
                self.indent();
                self.conditional_no_indent(cond);
            }
            BodyItem::Schema(schema) => {
                self.indent();
                self.out.push_str("schema ");
                self.string_lit(&schema.name);
                self.out.push_str(" {\n");
                self.indent += 1;
                for field in &schema.fields {
                    self.indent();
                    self.out.push_str(&format!("{} = ", field.name.name));
                    self.type_expr(&field.type_expr);
                    self.out.push('\n');
                }
                self.indent -= 1;
                self.indent();
                self.out.push_str("}\n");
            }
            BodyItem::Validation(val) => {
                self.indent();
                self.out.push_str("validation ");
                self.string_lit(&val.name);
                self.out.push_str(" {\n");
                self.indent += 1;
                for lb in &val.lets {
                    self.indent();
                    self.out.push_str(&format!("let {} = ", lb.name.name));
                    self.expr(&lb.value);
                    self.out.push('\n');
                }
                self.indent();
                self.out.push_str("check = ");
                self.expr(&val.check);
                self.out.push('\n');
                self.indent();
                self.out.push_str("message = ");
                self.expr(&val.message);
                self.out.push('\n');
                self.indent -= 1;
                self.indent();
                self.out.push_str("}\n");
            }
            BodyItem::MacroDef(md) => {
                for dec in &md.decorators {
                    self.decorator(dec);
                }
                self.indent();
                match md.kind {
                    MacroKind::Function => {
                        self.out.push_str(&format!("macro {}(", md.name.name));
                    }
                    MacroKind::Attribute => {
                        self.out.push_str(&format!("macro @{}(", md.name.name));
                    }
                }
                self.macro_params(&md.params);
                self.out.push_str(") {\n");
                self.indent += 1;
                match &md.body {
                    MacroBody::Function(items) => {
                        for child in items {
                            self.body_item(child);
                        }
                    }
                    MacroBody::Attribute(directives) => {
                        for directive in directives {
                            self.transform_directive(directive);
                        }
                    }
                }
                self.indent -= 1;
                self.indent();
                self.out.push_str("}\n");
            }
            BodyItem::MacroCall(mc) => {
                self.indent();
                self.out.push_str(&format!("{}(", mc.name.name));
                for (i, arg) in mc.args.iter().enumerate() {
                    if i > 0 {
                        self.out.push_str(", ");
                    }
                    match arg {
                        MacroCallArg::Positional(e) => self.expr(e),
                        MacroCallArg::Named(name, e) => {
                            self.out.push_str(&format!("{} = ", name.name));
                            self.expr(e);
                        }
                    }
                }
                self.out.push_str(")\n");
            }
            BodyItem::DecoratorSchema(ds) => {
                for dec in &ds.decorators {
                    self.decorator(dec);
                }
                self.indent();
                self.out.push_str("decorator_schema ");
                self.string_lit(&ds.name);
                self.out.push_str(" {\n");
                self.indent += 1;
                if !ds.target.is_empty() {
                    self.indent();
                    self.out.push_str("target = [");
                    for (i, t) in ds.target.iter().enumerate() {
                        if i > 0 {
                            self.out.push_str(", ");
                        }
                        let name = match t {
                            DecoratorTarget::Block => "block",
                            DecoratorTarget::Attribute => "attribute",
                            DecoratorTarget::Table => "table",
                            DecoratorTarget::Schema => "schema",
                        };
                        self.out.push_str(name);
                    }
                    self.out.push_str("]\n");
                }
                for field in &ds.fields {
                    self.schema_field(field);
                }
                self.indent -= 1;
                self.indent();
                self.out.push_str("}\n");
            }
        }
    }

    fn conditional_no_indent(&mut self, cond: &Conditional) {
        self.out.push_str("if ");
        self.expr(&cond.condition);
        self.out.push_str(" {\n");
        self.indent += 1;
        for child in &cond.then_body {
            self.body_item(child);
        }
        self.indent -= 1;
        self.indent();
        self.out.push('}');
        match &cond.else_branch {
            Some(ElseBranch::ElseIf(inner)) => {
                self.out.push_str(" else ");
                self.conditional_no_indent(inner);
            }
            Some(ElseBranch::Else(body, _, _)) => {
                self.out.push_str(" else {\n");
                self.indent += 1;
                for child in body {
                    self.body_item(child);
                }
                self.indent -= 1;
                self.indent();
                self.out.push_str("}\n");
            }
            None => {
                self.out.push('\n');
            }
        }
    }

    fn decorator(&mut self, dec: &Decorator) {
        self.indent();
        self.out.push('@');
        self.out.push_str(&dec.name.name);
        if !dec.args.is_empty() {
            self.out.push('(');
            for (i, arg) in dec.args.iter().enumerate() {
                if i > 0 {
                    self.out.push_str(", ");
                }
                match arg {
                    DecoratorArg::Positional(e) => self.expr(e),
                    DecoratorArg::Named(name, e) => {
                        self.out.push_str(&format!("{} = ", name.name));
                        self.expr(e);
                    }
                }
            }
            self.out.push(')');
        }
        self.out.push('\n');
    }

    fn expr(&mut self, expr: &Expr) {
        match expr {
            Expr::IntLit(i, _) => self.out.push_str(&i.to_string()),
            Expr::FloatLit(f, _) => self.out.push_str(&f.to_string()),
            Expr::BoolLit(b, _) => self.out.push_str(if *b { "true" } else { "false" }),
            Expr::NullLit(_) => self.out.push_str("null"),
            Expr::StringLit(s) => self.string_lit(s),
            Expr::Ident(id) => self.out.push_str(&id.name),
            Expr::IdentifierLit(id) => self.out.push_str(&id.value),
            Expr::List(items, _) => {
                self.out.push('[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        self.out.push_str(", ");
                    }
                    self.expr(item);
                }
                self.out.push(']');
            }
            Expr::Map(entries, _) => {
                self.out.push_str("{ ");
                for (i, (key, val)) in entries.iter().enumerate() {
                    if i > 0 {
                        self.out.push_str(", ");
                    }
                    match key {
                        MapKey::Ident(id) => self.out.push_str(&id.name),
                        MapKey::String(s) => self.string_lit(s),
                    }
                    self.out.push_str(" = ");
                    self.expr(val);
                }
                self.out.push_str(" }");
            }
            Expr::BinaryOp(lhs, op, rhs, _) => {
                self.expr(lhs);
                let op_str = match op {
                    BinOp::Add => " + ",
                    BinOp::Sub => " - ",
                    BinOp::Mul => " * ",
                    BinOp::Div => " / ",
                    BinOp::Mod => " % ",
                    BinOp::Eq => " == ",
                    BinOp::Neq => " != ",
                    BinOp::Lt => " < ",
                    BinOp::Gt => " > ",
                    BinOp::Lte => " <= ",
                    BinOp::Gte => " >= ",
                    BinOp::Match => " =~ ",
                    BinOp::And => " && ",
                    BinOp::Or => " || ",
                };
                self.out.push_str(op_str);
                self.expr(rhs);
            }
            Expr::UnaryOp(op, e, _) => {
                match op {
                    UnaryOp::Not => self.out.push('!'),
                    UnaryOp::Neg => self.out.push('-'),
                }
                self.expr(e);
            }
            Expr::Ternary(cond, then_expr, else_expr, _) => {
                self.expr(cond);
                self.out.push_str(" ? ");
                self.expr(then_expr);
                self.out.push_str(" : ");
                self.expr(else_expr);
            }
            Expr::MemberAccess(e, field, _) => {
                self.expr(e);
                self.out.push('.');
                self.out.push_str(&field.name);
            }
            Expr::IndexAccess(e, idx, _) => {
                self.expr(e);
                self.out.push('[');
                self.expr(idx);
                self.out.push(']');
            }
            Expr::FnCall(callee, args, _) => {
                self.expr(callee);
                self.out.push('(');
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        self.out.push_str(", ");
                    }
                    match arg {
                        CallArg::Positional(e) => self.expr(e),
                        CallArg::Named(name, e) => {
                            self.out.push_str(&format!("{} = ", name.name));
                            self.expr(e);
                        }
                    }
                }
                self.out.push(')');
            }
            Expr::Lambda(params, body, _) => {
                if params.len() == 1 {
                    self.out.push_str(&params[0].name);
                } else {
                    self.out.push('(');
                    for (i, p) in params.iter().enumerate() {
                        if i > 0 {
                            self.out.push_str(", ");
                        }
                        self.out.push_str(&p.name);
                    }
                    self.out.push(')');
                }
                self.out.push_str(" => ");
                self.expr(body);
            }
            Expr::Paren(e, _) => {
                self.out.push('(');
                self.expr(e);
                self.out.push(')');
            }
            Expr::Ref(id, _) => {
                self.out.push_str(&format!("ref({})", id.value));
            }
            _ => {
                self.out.push_str("/* expr */");
            }
        }
    }

    fn string_lit(&mut self, s: &StringLit) {
        self.out.push('"');
        for part in &s.parts {
            match part {
                StringPart::Literal(text) => {
                    for c in text.chars() {
                        match c {
                            '"' => self.out.push_str("\\\""),
                            '\\' => self.out.push_str("\\\\"),
                            '\n' => self.out.push_str("\\n"),
                            '\r' => self.out.push_str("\\r"),
                            '\t' => self.out.push_str("\\t"),
                            c => self.out.push(c),
                        }
                    }
                }
                StringPart::Interpolation(e) => {
                    self.out.push_str("${");
                    self.expr(e);
                    self.out.push('}');
                }
            }
        }
        self.out.push('"');
    }

    fn type_expr(&mut self, te: &TypeExpr) {
        match te {
            TypeExpr::String(_) => self.out.push_str("string"),
            TypeExpr::Int(_) => self.out.push_str("int"),
            TypeExpr::Float(_) => self.out.push_str("float"),
            TypeExpr::Bool(_) => self.out.push_str("bool"),
            TypeExpr::Null(_) => self.out.push_str("null"),
            TypeExpr::Identifier(_) => self.out.push_str("identifier"),
            TypeExpr::Any(_) => self.out.push_str("any"),
            TypeExpr::List(inner, _) => {
                self.out.push_str("list(");
                self.type_expr(inner);
                self.out.push(')');
            }
            TypeExpr::Map(k, v, _) => {
                self.out.push_str("map(");
                self.type_expr(k);
                self.out.push_str(", ");
                self.type_expr(v);
                self.out.push(')');
            }
            TypeExpr::Set(inner, _) => {
                self.out.push_str("set(");
                self.type_expr(inner);
                self.out.push(')');
            }
            TypeExpr::Ref(name, _) => {
                self.out.push_str("ref(");
                self.string_lit(name);
                self.out.push(')');
            }
            TypeExpr::Union(types, _) => {
                self.out.push_str("union(");
                for (i, t) in types.iter().enumerate() {
                    if i > 0 {
                        self.out.push_str(", ");
                    }
                    self.type_expr(t);
                }
                self.out.push(')');
            }
        }
    }

    fn macro_params(&mut self, params: &[MacroParam]) {
        for (i, p) in params.iter().enumerate() {
            if i > 0 {
                self.out.push_str(", ");
            }
            self.out.push_str(&p.name.name);
            if let Some(tc) = &p.type_constraint {
                self.out.push_str(": ");
                self.type_expr(tc);
            }
            if let Some(def) = &p.default {
                self.out.push_str(" = ");
                self.expr(def);
            }
        }
    }

    fn transform_directive(&mut self, directive: &TransformDirective) {
        match directive {
            TransformDirective::Inject(inject) => {
                self.indent();
                self.out.push_str("inject {\n");
                self.indent += 1;
                for child in &inject.body {
                    self.body_item(child);
                }
                self.indent -= 1;
                self.indent();
                self.out.push_str("}\n");
            }
            TransformDirective::Set(set) => {
                self.indent();
                self.out.push_str("set {\n");
                self.indent += 1;
                for attr in &set.attrs {
                    self.indent();
                    self.out.push_str(&format!("{} = ", attr.name.name));
                    self.expr(&attr.value);
                    self.out.push('\n');
                }
                self.indent -= 1;
                self.indent();
                self.out.push_str("}\n");
            }
            TransformDirective::Remove(remove) => {
                self.indent();
                self.out.push_str("remove [");
                for (i, target) in remove.targets.iter().enumerate() {
                    if i > 0 {
                        self.out.push_str(", ");
                    }
                    self.format_remove_target(target);
                }
                self.out.push_str("]\n");
            }
            TransformDirective::Update(update) => {
                self.indent();
                self.out.push_str("update ");
                self.format_target_selector(&update.selector);
                self.out.push_str(" {\n");
                self.indent += 1;
                for d in &update.block_directives {
                    self.transform_directive(d);
                }
                for d in &update.table_directives {
                    self.format_table_directive(d);
                }
                self.indent -= 1;
                self.indent();
                self.out.push_str("}\n");
            }
            TransformDirective::When(when) => {
                self.indent();
                self.out.push_str("when ");
                self.expr(&when.condition);
                self.out.push_str(" {\n");
                self.indent += 1;
                for d in &when.directives {
                    self.transform_directive(d);
                }
                self.indent -= 1;
                self.indent();
                self.out.push_str("}\n");
            }
        }
    }

    fn format_remove_target(&mut self, target: &RemoveTarget) {
        match target {
            RemoveTarget::Attr(ident) => self.out.push_str(&ident.name),
            RemoveTarget::Block(kind, id) => {
                self.out.push_str(&kind.name);
                self.out.push('#');
                self.out.push_str(&id.value);
            }
            RemoveTarget::BlockAll(kind) => {
                self.out.push_str(&kind.name);
                self.out.push_str("#*");
            }
            RemoveTarget::BlockIndex(kind, n, _) => {
                self.out.push_str(&format!("{}[{}]", kind.name, n));
            }
            RemoveTarget::Table(id) => {
                self.out.push_str("table#");
                self.out.push_str(&id.value);
            }
            RemoveTarget::AllTables(_) => {
                self.out.push_str("table#*");
            }
            RemoveTarget::TableIndex(n, _) => {
                self.out.push_str(&format!("table[{}]", n));
            }
        }
    }

    fn format_target_selector(&mut self, selector: &TargetSelector) {
        match selector {
            TargetSelector::BlockKind(kind) => self.out.push_str(&kind.name),
            TargetSelector::BlockKindId(kind, id) => {
                self.out.push_str(&kind.name);
                self.out.push('#');
                self.out.push_str(&id.value);
            }
            TargetSelector::BlockIndex(kind, n, _) => {
                self.out.push_str(&format!("{}[{}]", kind.name, n));
            }
            TargetSelector::TableId(id) => {
                self.out.push_str("table#");
                self.out.push_str(&id.value);
            }
            TargetSelector::TableIndex(n, _) => {
                self.out.push_str(&format!("table[{}]", n));
            }
        }
    }

    fn format_table_directive(&mut self, directive: &TableDirective) {
        match directive {
            TableDirective::InjectRows(rows, _) => {
                self.indent();
                self.out.push_str("inject_rows {\n");
                self.indent += 1;
                for row in rows {
                    self.indent();
                    for cell in &row.cells {
                        self.out.push_str("| ");
                        self.expr(cell);
                        self.out.push(' ');
                    }
                    self.out.push_str("|\n");
                }
                self.indent -= 1;
                self.indent();
                self.out.push_str("}\n");
            }
            TableDirective::RemoveRows { condition, .. } => {
                self.indent();
                self.out.push_str("remove_rows where ");
                self.expr(condition);
                self.out.push('\n');
            }
            TableDirective::UpdateRows {
                condition, attrs, ..
            } => {
                self.indent();
                self.out.push_str("update_rows where ");
                self.expr(condition);
                self.out.push_str(" {\n");
                self.indent += 1;
                self.indent();
                self.out.push_str("set {\n");
                self.indent += 1;
                for (name, val) in attrs {
                    self.indent();
                    self.out.push_str(&format!("{} = ", name.name));
                    self.expr(val);
                    self.out.push('\n');
                }
                self.indent -= 1;
                self.indent();
                self.out.push_str("}\n");
                self.indent -= 1;
                self.indent();
                self.out.push_str("}\n");
            }
            TableDirective::ClearRows(_) => {
                self.indent();
                self.out.push_str("clear_rows\n");
            }
        }
    }

    fn schema_field(&mut self, field: &SchemaField) {
        for dec in &field.decorators_before {
            self.decorator(dec);
        }
        self.indent();
        self.out.push_str(&format!("{} = ", field.name.name));
        self.type_expr(&field.type_expr);
        for dec in &field.decorators_after {
            self.out.push(' ');
            self.decorator_inline(dec);
        }
        self.out.push('\n');
    }

    fn decorator_inline(&mut self, dec: &Decorator) {
        self.out.push('@');
        self.out.push_str(&dec.name.name);
        if !dec.args.is_empty() {
            self.out.push('(');
            for (i, arg) in dec.args.iter().enumerate() {
                if i > 0 {
                    self.out.push_str(", ");
                }
                match arg {
                    DecoratorArg::Positional(e) => self.expr(e),
                    DecoratorArg::Named(name, e) => {
                        self.out.push_str(&format!("{} = ", name.name));
                        self.expr(e);
                    }
                }
            }
            self.out.push(')');
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wcl_core::span::Span;
    use wcl_core::trivia::Trivia;

    fn dummy_span() -> Span {
        Span::dummy()
    }

    fn dummy_trivia() -> Trivia {
        Trivia::default()
    }

    fn make_ident(name: &str) -> Ident {
        Ident {
            name: name.to_string(),
            span: dummy_span(),
        }
    }

    fn make_string_lit(s: &str) -> StringLit {
        StringLit {
            parts: vec![StringPart::Literal(s.to_string())],
            span: dummy_span(),
        }
    }

    #[test]
    fn test_format_function_macro() {
        let macro_def = MacroDef {
            decorators: vec![],
            kind: MacroKind::Function,
            name: make_ident("my_macro"),
            params: vec![
                MacroParam {
                    name: make_ident("x"),
                    type_constraint: Some(TypeExpr::Int(dummy_span())),
                    default: None,
                    span: dummy_span(),
                },
                MacroParam {
                    name: make_ident("y"),
                    type_constraint: None,
                    default: Some(Expr::IntLit(42, dummy_span())),
                    span: dummy_span(),
                },
            ],
            body: MacroBody::Function(vec![BodyItem::Attribute(Attribute {
                decorators: vec![],
                name: make_ident("value"),
                value: Expr::Ident(make_ident("x")),
                trivia: dummy_trivia(),
                span: dummy_span(),
            })]),
            trivia: dummy_trivia(),
            span: dummy_span(),
        };
        let doc = Document {
            items: vec![DocItem::Body(BodyItem::MacroDef(macro_def))],
            trivia: dummy_trivia(),
            span: dummy_span(),
        };
        let result = format_document(&doc);
        assert_eq!(
            result,
            "macro my_macro(x: int, y = 42) {\n    value = x\n}\n\n"
        );
    }

    #[test]
    fn test_format_macro_call() {
        let mc = MacroCall {
            name: make_ident("my_macro"),
            args: vec![
                MacroCallArg::Positional(Expr::IntLit(1, dummy_span())),
                MacroCallArg::Named(make_ident("y"), Expr::IntLit(2, dummy_span())),
            ],
            trivia: dummy_trivia(),
            span: dummy_span(),
        };
        let doc = Document {
            items: vec![DocItem::Body(BodyItem::MacroCall(mc))],
            trivia: dummy_trivia(),
            span: dummy_span(),
        };
        let result = format_document(&doc);
        assert_eq!(result, "my_macro(1, y = 2)\n\n");
    }

    #[test]
    fn test_format_decorator_schema() {
        let ds = DecoratorSchema {
            decorators: vec![],
            name: make_string_lit("my_decorator"),
            target: vec![DecoratorTarget::Block, DecoratorTarget::Attribute],
            fields: vec![SchemaField {
                decorators_before: vec![],
                name: make_ident("level"),
                type_expr: TypeExpr::String(dummy_span()),
                decorators_after: vec![],
                trivia: dummy_trivia(),
                span: dummy_span(),
            }],
            trivia: dummy_trivia(),
            span: dummy_span(),
        };
        let doc = Document {
            items: vec![DocItem::Body(BodyItem::DecoratorSchema(ds))],
            trivia: dummy_trivia(),
            span: dummy_span(),
        };
        let result = format_document(&doc);
        assert_eq!(
            result,
            "decorator_schema \"my_decorator\" {\n    target = [block, attribute]\n    level = string\n}\n\n"
        );
    }
}
