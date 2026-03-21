use std::path::Path;
use wcl_core::ast::*;

pub fn run(file: &Path, write: bool, check: bool) -> Result<(), String> {
    let source = std::fs::read_to_string(file)
        .map_err(|e| format!("cannot read {}: {}", file.display(), e))?;

    let file_id = wcl_core::FileId(0);
    let (doc, diags) = wcl_core::parse(&source, file_id);

    if diags.has_errors() {
        for d in diags.diagnostics() {
            if d.is_error() {
                eprintln!("error: {}", d.message);
            }
        }
        return Err(format!("parse errors in {}", file.display()));
    }

    let formatted = format_document(&doc);

    if check {
        if formatted == source {
            Ok(())
        } else {
            Err(format!("{} is not formatted", file.display()))
        }
    } else if write {
        std::fs::write(file, &formatted)
            .map_err(|e| format!("cannot write {}: {}", file.display(), e))?;
        println!("formatted {}", file.display());
        Ok(())
    } else {
        print!("{}", formatted);
        Ok(())
    }
}

fn format_document(doc: &Document) -> String {
    let mut output = String::new();
    let mut formatter = Formatter {
        output: &mut output,
        indent: 0,
    };
    formatter.format_doc(doc);
    output
}

struct Formatter<'a> {
    output: &'a mut String,
    indent: usize,
}

impl<'a> Formatter<'a> {
    fn write_indent(&mut self) {
        for _ in 0..self.indent {
            self.output.push_str("    ");
        }
    }

    fn format_doc(&mut self, doc: &Document) {
        for (i, item) in doc.items.iter().enumerate() {
            if i > 0 {
                self.output.push('\n');
            }
            self.format_doc_item(item);
        }
        if !doc.items.is_empty() {
            self.output.push('\n');
        }
    }

    fn format_doc_item(&mut self, item: &DocItem) {
        match item {
            DocItem::Import(import) => {
                self.write_indent();
                self.output.push_str("import ");
                if import.kind == ImportKind::Library {
                    self.output.push('<');
                    for part in &import.path.parts {
                        if let StringPart::Literal(s) = part {
                            self.output.push_str(s);
                        }
                    }
                    self.output.push('>');
                } else {
                    self.format_string_lit(&import.path);
                }
                self.output.push('\n');
            }
            DocItem::ExportLet(el) => {
                self.write_indent();
                self.output
                    .push_str(&format!("export let {} = ", el.name.name));
                self.format_expr(&el.value);
                self.output.push('\n');
            }
            DocItem::ReExport(re) => {
                self.write_indent();
                self.output.push_str(&format!("export {}\n", re.name.name));
            }
            DocItem::Body(body_item) => {
                self.format_body_item(body_item);
            }
            DocItem::FunctionDecl(decl) => {
                self.write_indent();
                self.output
                    .push_str(&format!("declare {}(", decl.name.name));
                for (i, param) in decl.params.iter().enumerate() {
                    if i > 0 {
                        self.output.push_str(", ");
                    }
                    self.output.push_str(&format!("{}: ", param.name.name));
                    self.format_type_expr(&param.type_expr);
                }
                self.output.push(')');
                if let Some(ref rt) = decl.return_type {
                    self.output.push_str(" -> ");
                    self.format_type_expr(rt);
                }
                self.output.push('\n');
            }
        }
    }

    fn format_body_item(&mut self, item: &BodyItem) {
        match item {
            BodyItem::Attribute(attr) => {
                for dec in &attr.decorators {
                    self.format_decorator(dec);
                }
                self.write_indent();
                self.output.push_str(&format!("{} = ", attr.name.name));
                self.format_expr(&attr.value);
                self.output.push('\n');
            }
            BodyItem::Block(block) => {
                for dec in &block.decorators {
                    self.format_decorator(dec);
                }
                self.write_indent();
                if block.partial {
                    self.output.push_str("partial ");
                }
                self.output.push_str(&block.kind.name);
                if let Some(id) = &block.inline_id {
                    self.output.push(' ');
                    match id {
                        InlineId::Literal(lit) => self.output.push_str(&lit.value),
                        InlineId::Interpolated(parts) => {
                            for part in parts {
                                match part {
                                    StringPart::Literal(s) => self.output.push_str(s),
                                    StringPart::Interpolation(e) => {
                                        self.output.push_str("${");
                                        self.format_expr(e);
                                        self.output.push('}');
                                    }
                                }
                            }
                        }
                    }
                }
                for label in &block.labels {
                    self.output.push(' ');
                    self.format_string_lit(label);
                }
                if let Some(ref tc) = block.text_content {
                    self.output.push(' ');
                    self.format_string_lit(tc);
                    self.output.push('\n');
                } else {
                    self.output.push_str(" {\n");
                    self.indent += 1;
                    for child in &block.body {
                        self.format_body_item(child);
                    }
                    self.indent -= 1;
                    self.write_indent();
                    self.output.push_str("}\n");
                }
            }
            BodyItem::LetBinding(lb) => {
                self.write_indent();
                self.output.push_str(&format!("let {} = ", lb.name.name));
                self.format_expr(&lb.value);
                self.output.push('\n');
            }
            BodyItem::Table(table) => {
                self.write_indent();
                if table.partial {
                    self.output.push_str("partial ");
                }
                self.output.push_str("table");
                if let Some(id) = &table.inline_id {
                    self.output.push(' ');
                    match id {
                        InlineId::Literal(lit) => self.output.push_str(&lit.value),
                        InlineId::Interpolated(_) => self.output.push_str("<interpolated-id>"),
                    }
                }
                if let Some(ref sr) = table.schema_ref {
                    self.output.push_str(&format!(" : {}", sr.name));
                }
                if let Some(ref expr) = table.import_expr {
                    self.output.push_str(" = ");
                    self.format_expr(expr);
                    self.output.push('\n');
                } else {
                    self.output.push_str(" {\n");
                    self.indent += 1;
                    for col in &table.columns {
                        self.write_indent();
                        self.output.push_str(&format!("{} : ", col.name.name));
                        self.format_type_expr(&col.type_expr);
                        self.output.push('\n');
                    }
                    if !table.columns.is_empty() && !table.rows.is_empty() {
                        self.output.push('\n');
                    }
                    for row in &table.rows {
                        self.write_indent();
                        for cell in &row.cells {
                            self.output.push_str("| ");
                            self.format_expr(cell);
                            self.output.push(' ');
                        }
                        self.output.push_str("|\n");
                    }
                    self.indent -= 1;
                    self.write_indent();
                    self.output.push_str("}\n");
                }
            }
            BodyItem::ForLoop(fl) => {
                self.write_indent();
                self.output.push_str(&format!("for {}", fl.iterator.name));
                if let Some(idx) = &fl.index {
                    self.output.push_str(&format!(", {}", idx.name));
                }
                self.output.push_str(" in ");
                self.format_expr(&fl.iterable);
                self.output.push_str(" {\n");
                self.indent += 1;
                for child in &fl.body {
                    self.format_body_item(child);
                }
                self.indent -= 1;
                self.write_indent();
                self.output.push_str("}\n");
            }
            BodyItem::Conditional(cond) => {
                self.format_conditional(cond);
            }
            BodyItem::Schema(schema) => {
                self.write_indent();
                self.output.push_str("schema ");
                self.format_string_lit(&schema.name);
                self.output.push_str(" {\n");
                self.indent += 1;
                for field in &schema.fields {
                    self.write_indent();
                    self.output.push_str(&format!("{} = ", field.name.name));
                    self.format_type_expr(&field.type_expr);
                    for dec in &field.decorators_after {
                        self.output.push(' ');
                        self.format_decorator_inline(dec);
                    }
                    self.output.push('\n');
                }
                self.indent -= 1;
                self.write_indent();
                self.output.push_str("}\n");
            }
            BodyItem::Validation(val) => {
                self.write_indent();
                self.output.push_str("validation ");
                self.format_string_lit(&val.name);
                self.output.push_str(" {\n");
                self.indent += 1;
                for lb in &val.lets {
                    self.write_indent();
                    self.output.push_str(&format!("let {} = ", lb.name.name));
                    self.format_expr(&lb.value);
                    self.output.push('\n');
                }
                self.write_indent();
                self.output.push_str("check = ");
                self.format_expr(&val.check);
                self.output.push('\n');
                self.write_indent();
                self.output.push_str("message = ");
                self.format_expr(&val.message);
                self.output.push('\n');
                self.indent -= 1;
                self.write_indent();
                self.output.push_str("}\n");
            }
            BodyItem::MacroDef(_) | BodyItem::MacroCall(_) | BodyItem::DecoratorSchema(_) => {
                self.write_indent();
                self.output.push_str("// <unformatted item>\n");
            }
        }
    }

    fn format_conditional(&mut self, cond: &Conditional) {
        self.write_indent();
        self.format_conditional_no_indent(cond);
    }

    fn format_conditional_no_indent(&mut self, cond: &Conditional) {
        self.output.push_str("if ");
        self.format_expr(&cond.condition);
        self.output.push_str(" {\n");
        self.indent += 1;
        for child in &cond.then_body {
            self.format_body_item(child);
        }
        self.indent -= 1;
        self.write_indent();
        self.output.push('}');
        match &cond.else_branch {
            Some(ElseBranch::ElseIf(inner)) => {
                self.output.push_str(" else ");
                self.format_conditional_no_indent(inner);
            }
            Some(ElseBranch::Else(body, _, _)) => {
                self.output.push_str(" else {\n");
                self.indent += 1;
                for child in body {
                    self.format_body_item(child);
                }
                self.indent -= 1;
                self.write_indent();
                self.output.push_str("}\n");
            }
            None => {
                self.output.push('\n');
            }
        }
    }

    fn format_decorator(&mut self, dec: &Decorator) {
        self.write_indent();
        self.format_decorator_inline(dec);
        self.output.push('\n');
    }

    fn format_decorator_inline(&mut self, dec: &Decorator) {
        self.output.push('@');
        self.output.push_str(&dec.name.name);
        if !dec.args.is_empty() {
            self.output.push('(');
            for (i, arg) in dec.args.iter().enumerate() {
                if i > 0 {
                    self.output.push_str(", ");
                }
                match arg {
                    DecoratorArg::Positional(e) => self.format_expr(e),
                    DecoratorArg::Named(name, e) => {
                        self.output.push_str(&format!("{} = ", name.name));
                        self.format_expr(e);
                    }
                }
            }
            self.output.push(')');
        }
    }

    fn format_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::IntLit(i, _) => self.output.push_str(&i.to_string()),
            Expr::FloatLit(f, _) => self.output.push_str(&f.to_string()),
            Expr::BoolLit(b, _) => self.output.push_str(if *b { "true" } else { "false" }),
            Expr::NullLit(_) => self.output.push_str("null"),
            Expr::StringLit(s) => self.format_string_lit(s),
            Expr::Ident(id) => self.output.push_str(&id.name),
            Expr::IdentifierLit(id) => self.output.push_str(&id.value),
            Expr::List(items, _) => {
                self.output.push('[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        self.output.push_str(", ");
                    }
                    self.format_expr(item);
                }
                self.output.push(']');
            }
            Expr::Map(entries, _) => {
                self.output.push_str("{ ");
                for (i, (key, val)) in entries.iter().enumerate() {
                    if i > 0 {
                        self.output.push_str(", ");
                    }
                    match key {
                        MapKey::Ident(id) => self.output.push_str(&id.name),
                        MapKey::String(s) => self.format_string_lit(s),
                    }
                    self.output.push_str(" = ");
                    self.format_expr(val);
                }
                self.output.push_str(" }");
            }
            Expr::BinaryOp(lhs, op, rhs, _) => {
                self.format_expr(lhs);
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
                self.output.push_str(op_str);
                self.format_expr(rhs);
            }
            Expr::UnaryOp(op, e, _) => {
                match op {
                    UnaryOp::Not => self.output.push('!'),
                    UnaryOp::Neg => self.output.push('-'),
                }
                self.format_expr(e);
            }
            Expr::Ternary(cond, then_expr, else_expr, _) => {
                self.format_expr(cond);
                self.output.push_str(" ? ");
                self.format_expr(then_expr);
                self.output.push_str(" : ");
                self.format_expr(else_expr);
            }
            Expr::MemberAccess(e, field, _) => {
                self.format_expr(e);
                self.output.push('.');
                self.output.push_str(&field.name);
            }
            Expr::IndexAccess(e, idx, _) => {
                self.format_expr(e);
                self.output.push('[');
                self.format_expr(idx);
                self.output.push(']');
            }
            Expr::FnCall(callee, args, _) => {
                self.format_expr(callee);
                self.output.push('(');
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        self.output.push_str(", ");
                    }
                    match arg {
                        CallArg::Positional(e) => self.format_expr(e),
                        CallArg::Named(name, e) => {
                            self.output.push_str(&format!("{} = ", name.name));
                            self.format_expr(e);
                        }
                    }
                }
                self.output.push(')');
            }
            Expr::Lambda(params, body, _) => {
                if params.len() == 1 {
                    self.output.push_str(&params[0].name);
                } else {
                    self.output.push('(');
                    for (i, p) in params.iter().enumerate() {
                        if i > 0 {
                            self.output.push_str(", ");
                        }
                        self.output.push_str(&p.name);
                    }
                    self.output.push(')');
                }
                self.output.push_str(" => ");
                self.format_expr(body);
            }
            Expr::Paren(e, _) => {
                self.output.push('(');
                self.format_expr(e);
                self.output.push(')');
            }
            Expr::Query(_, _) => {
                self.output.push_str("query(...)");
            }
            Expr::Ref(id, _) => {
                self.output.push_str(&format!("ref({})", id.value));
            }
            Expr::ImportRaw(path, _) => {
                self.output.push_str("import_raw(");
                self.format_string_lit(path);
                self.output.push(')');
            }
            Expr::ImportTable(args, _) => {
                self.output.push_str("import_table(");
                self.format_string_lit(&args.path);
                if let Some(ref sep) = args.separator {
                    self.output.push_str(", separator = ");
                    self.format_string_lit(sep);
                }
                if let Some(h) = args.headers {
                    self.output
                        .push_str(&format!(", headers = {}", if h { "true" } else { "false" }));
                }
                if let Some(ref cols) = args.columns {
                    self.output.push_str(", columns = [");
                    for (i, c) in cols.iter().enumerate() {
                        if i > 0 {
                            self.output.push_str(", ");
                        }
                        self.format_string_lit(c);
                    }
                    self.output.push(']');
                }
                self.output.push(')');
            }
            _ => {
                self.output.push_str("/* expr */");
            }
        }
    }

    fn format_string_lit(&mut self, s: &StringLit) {
        self.output.push('"');
        for part in &s.parts {
            match part {
                StringPart::Literal(text) => {
                    for c in text.chars() {
                        match c {
                            '"' => self.output.push_str("\\\""),
                            '\\' => self.output.push_str("\\\\"),
                            '\n' => self.output.push_str("\\n"),
                            '\r' => self.output.push_str("\\r"),
                            '\t' => self.output.push_str("\\t"),
                            c => self.output.push(c),
                        }
                    }
                }
                StringPart::Interpolation(e) => {
                    self.output.push_str("${");
                    self.format_expr(e);
                    self.output.push('}');
                }
            }
        }
        self.output.push('"');
    }

    fn format_type_expr(&mut self, te: &TypeExpr) {
        match te {
            TypeExpr::String(_) => self.output.push_str("string"),
            TypeExpr::Int(_) => self.output.push_str("int"),
            TypeExpr::Float(_) => self.output.push_str("float"),
            TypeExpr::Bool(_) => self.output.push_str("bool"),
            TypeExpr::Null(_) => self.output.push_str("null"),
            TypeExpr::Identifier(_) => self.output.push_str("identifier"),
            TypeExpr::Any(_) => self.output.push_str("any"),
            TypeExpr::List(inner, _) => {
                self.output.push_str("list(");
                self.format_type_expr(inner);
                self.output.push(')');
            }
            TypeExpr::Map(k, v, _) => {
                self.output.push_str("map(");
                self.format_type_expr(k);
                self.output.push_str(", ");
                self.format_type_expr(v);
                self.output.push(')');
            }
            TypeExpr::Set(inner, _) => {
                self.output.push_str("set(");
                self.format_type_expr(inner);
                self.output.push(')');
            }
            TypeExpr::Ref(name, _) => {
                self.output.push_str("ref(");
                self.format_string_lit(name);
                self.output.push(')');
            }
            TypeExpr::Union(types, _) => {
                self.output.push_str("union(");
                for (i, t) in types.iter().enumerate() {
                    if i > 0 {
                        self.output.push_str(", ");
                    }
                    self.format_type_expr(t);
                }
                self.output.push(')');
            }
        }
    }
}
