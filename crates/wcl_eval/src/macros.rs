use crate::value::Value;
use std::collections::HashMap;
use wcl_core::ast::*;
use wcl_core::diagnostic::DiagnosticBag;
use wcl_core::span::Span;

/// Registry of macro definitions collected from the document.
///
/// Function macros produce AST fragments spliced at the call site.
/// Attribute macros transform the block they are attached to via decorators.
#[derive(Debug, Default)]
pub struct MacroRegistry {
    /// Function macros keyed by name.
    pub function_macros: HashMap<String, MacroDef>,
    /// Attribute macros keyed by name (without the `@` prefix).
    pub attribute_macros: HashMap<String, MacroDef>,
}

impl MacroRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Collect macro definitions from the document, removing them from the AST.
    ///
    /// Walks `doc.items`, finds `BodyItem::MacroDef` entries, registers each
    /// macro in the appropriate map (function or attribute), checks for
    /// duplicates, and removes the `MacroDef` items from the document.
    pub fn collect(&mut self, doc: &mut Document, diagnostics: &mut DiagnosticBag) {
        let mut retained_items: Vec<DocItem> = Vec::with_capacity(doc.items.len());

        for item in doc.items.drain(..) {
            match item {
                DocItem::Body(BodyItem::MacroDef(ref def)) => {
                    let name = def.name.name.clone();
                    let span = def.span;

                    match def.kind {
                        MacroKind::Function => {
                            if let std::collections::hash_map::Entry::Vacant(e) =
                                self.function_macros.entry(name.clone())
                            {
                                if let DocItem::Body(BodyItem::MacroDef(def)) = item {
                                    e.insert(def);
                                }
                                continue;
                            } else {
                                diagnostics.error(
                                    format!("duplicate function macro definition: '{}'", name),
                                    span,
                                );
                            }
                        }
                        MacroKind::Attribute => {
                            if let std::collections::hash_map::Entry::Vacant(e) =
                                self.attribute_macros.entry(name.clone())
                            {
                                if let DocItem::Body(BodyItem::MacroDef(def)) = item {
                                    e.insert(def);
                                }
                                continue;
                            } else {
                                diagnostics.error(
                                    format!("duplicate attribute macro definition: '@{}'", name),
                                    span,
                                );
                            }
                        }
                    }
                    // If we got here, it was a duplicate — retain the item for error reporting
                    // but the original is already registered so we just drop the duplicate
                }
                _ => {
                    retained_items.push(item);
                }
            }
        }

        doc.items = retained_items;
    }

    /// Check whether a function macro with the given name is registered.
    pub fn has_function_macro(&self, name: &str) -> bool {
        self.function_macros.contains_key(name)
    }

    /// Check whether an attribute macro with the given name is registered.
    pub fn has_attribute_macro(&self, name: &str) -> bool {
        self.attribute_macros.contains_key(name)
    }
}

/// Expands macro calls in a WCL document.
///
/// Iterates until no macro calls remain (fixed-point expansion), with a
/// configurable depth limit to prevent infinite expansion from recursive macros.
pub struct MacroExpander<'a> {
    registry: &'a MacroRegistry,
    expansion_stack: Vec<String>,
    max_depth: u32,
    diagnostics: DiagnosticBag,
}

impl<'a> MacroExpander<'a> {
    pub fn new(registry: &'a MacroRegistry, max_depth: u32) -> Self {
        MacroExpander {
            registry,
            expansion_stack: Vec::new(),
            max_depth,
            diagnostics: DiagnosticBag::new(),
        }
    }

    /// Expand all macro calls in the document.
    ///
    /// Iterates until no macro calls remain or the iteration limit is reached.
    pub fn expand(&mut self, doc: &mut Document) {
        let mut changed = true;
        let mut iterations: u32 = 0;
        while changed && iterations < self.max_depth {
            changed = false;
            changed |= self.expand_doc_items(&mut doc.items);
            iterations += 1;
        }
        if iterations >= self.max_depth && changed {
            self.diagnostics.error_with_code(
                format!(
                    "macro expansion did not converge after {} iterations",
                    self.max_depth
                ),
                Span::dummy(),
                "E022",
            );
        }
    }

    /// Walk document items, expanding function macro calls inline and
    /// applying attribute macro decorators on blocks.
    fn expand_doc_items(&mut self, items: &mut Vec<DocItem>) -> bool {
        let mut changed = false;
        let mut i = 0;
        while i < items.len() {
            match &mut items[i] {
                DocItem::Body(BodyItem::MacroCall(call)) => {
                    let call_clone = call.clone();
                    if let Some(expanded) = self.expand_function_macro(&call_clone) {
                        items.remove(i);
                        for (offset, body_item) in expanded.into_iter().enumerate() {
                            items.insert(i + offset, DocItem::Body(body_item));
                        }
                        changed = true;
                        // Don't increment i — re-check the newly inserted items
                        continue;
                    }
                    i += 1;
                }
                DocItem::Body(BodyItem::Block(block)) => {
                    // Check for attribute macro decorators
                    changed |= self.apply_attribute_macros(block);
                    // Recurse into block body
                    changed |= self.expand_body_items(&mut block.body);
                    i += 1;
                }
                _ => {
                    i += 1;
                }
            }
        }
        changed
    }

    /// Expand macro calls within a list of body items.
    fn expand_body_items(&mut self, items: &mut Vec<BodyItem>) -> bool {
        let mut changed = false;
        let mut i = 0;
        while i < items.len() {
            match &mut items[i] {
                BodyItem::MacroCall(call) => {
                    let call_clone = call.clone();
                    if let Some(expanded) = self.expand_function_macro(&call_clone) {
                        items.remove(i);
                        for (offset, body_item) in expanded.into_iter().enumerate() {
                            items.insert(i + offset, body_item);
                        }
                        changed = true;
                        continue;
                    }
                    i += 1;
                }
                BodyItem::Block(block) => {
                    changed |= self.apply_attribute_macros(block);
                    changed |= self.expand_body_items(&mut block.body);
                    i += 1;
                }
                BodyItem::ForLoop(for_loop) => {
                    changed |= self.expand_body_items(&mut for_loop.body);
                    i += 1;
                }
                BodyItem::Conditional(cond) => {
                    changed |= self.expand_body_items(&mut cond.then_body);
                    if let Some(else_branch) = &mut cond.else_branch {
                        changed |= self.expand_else_branch(else_branch);
                    }
                    i += 1;
                }
                _ => {
                    i += 1;
                }
            }
        }
        changed
    }

    /// Expand macro calls within an else branch.
    fn expand_else_branch(&mut self, branch: &mut ElseBranch) -> bool {
        match branch {
            ElseBranch::ElseIf(cond) => {
                let mut changed = self.expand_body_items(&mut cond.then_body);
                if let Some(else_branch) = &mut cond.else_branch {
                    changed |= self.expand_else_branch(else_branch);
                }
                changed
            }
            ElseBranch::Else(body, _, _) => self.expand_body_items(body),
        }
    }

    /// Expand a function macro call, returning the expanded body items.
    ///
    /// Returns `None` if the macro is not found or recursion is detected.
    fn expand_function_macro(&mut self, call: &MacroCall) -> Option<Vec<BodyItem>> {
        let name = &call.name.name;

        let def = match self.registry.function_macros.get(name) {
            Some(def) => def,
            None => {
                self.diagnostics.error_with_code(
                    format!("undefined macro: '{}'", name),
                    call.span,
                    "E020",
                );
                return None;
            }
        };

        // Check for recursion
        if self.expansion_stack.contains(name) {
            self.diagnostics.error_with_code(
                format!(
                    "recursive macro expansion detected: '{}' (stack: {})",
                    name,
                    self.expansion_stack.join(" -> ")
                ),
                call.span,
                "E021",
            );
            return None;
        }

        // Check expansion depth
        if self.expansion_stack.len() as u32 >= self.max_depth {
            self.diagnostics.error_with_code(
                format!(
                    "macro expansion depth limit exceeded (max {})",
                    self.max_depth
                ),
                call.span,
                "E022",
            );
            return None;
        }

        // Bind parameters
        let param_bindings = match self.bind_params(&def.params, &call.args, call.span) {
            Ok(bindings) => bindings,
            Err(()) => return None,
        };

        // Clone and substitute
        let body = match &def.body {
            MacroBody::Function(items) => items.clone(),
            MacroBody::Attribute(_) => {
                self.diagnostics.error_with_code(
                    format!("cannot call attribute macro '{}' as a function macro", name),
                    call.span,
                    "E023",
                );
                return None;
            }
        };

        let expanded = self.substitute_params(&body, &param_bindings);

        self.expansion_stack.push(name.clone());
        // The expanded items may contain further macro calls, which will be
        // handled by the next iteration of the fixed-point loop.
        self.expansion_stack.pop();

        Some(expanded)
    }

    /// Apply any attribute macros found in a block's decorators.
    ///
    /// Returns true if any attribute macros were applied.
    fn apply_attribute_macros(&mut self, block: &mut Block) -> bool {
        let mut changed = false;
        let mut i = 0;
        while i < block.decorators.len() {
            let decorator_name = block.decorators[i].name.name.clone();
            if self.registry.attribute_macros.contains_key(&decorator_name) {
                let decorator = block.decorators.remove(i);
                changed |= self.apply_attribute_macro(block, &decorator_name, &decorator);
                // Don't increment — decorators shifted
            } else {
                i += 1;
            }
        }
        changed
    }

    /// Apply an attribute macro to a block using the given decorator invocation.
    fn apply_attribute_macro(
        &mut self,
        block: &mut Block,
        decorator_name: &str,
        decorator: &Decorator,
    ) -> bool {
        let def = match self.registry.attribute_macros.get(decorator_name) {
            Some(def) => def.clone(),
            None => return false,
        };

        // Convert decorator args to MacroCallArgs for parameter binding
        let call_args: Vec<MacroCallArg> = decorator
            .args
            .iter()
            .map(|arg| match arg {
                DecoratorArg::Positional(expr) => MacroCallArg::Positional(expr.clone()),
                DecoratorArg::Named(ident, expr) => {
                    MacroCallArg::Named(ident.clone(), expr.clone())
                }
            })
            .collect();

        let param_bindings = match self.bind_params(&def.params, &call_args, decorator.span) {
            Ok(bindings) => bindings,
            Err(()) => return false,
        };

        let directives = match &def.body {
            MacroBody::Attribute(directives) => directives.clone(),
            MacroBody::Function(_) => {
                self.diagnostics.error_with_code(
                    format!(
                        "cannot apply function macro '{}' as an attribute macro",
                        decorator_name
                    ),
                    decorator.span,
                    "E023",
                );
                return false;
            }
        };

        // Apply each transform directive
        let mut changed = false;
        for directive in &directives {
            changed |= self.apply_directive(block, directive, &param_bindings);
        }

        changed
    }

    /// Apply a single transform directive to a block.
    fn apply_directive(
        &mut self,
        block: &mut Block,
        directive: &TransformDirective,
        param_bindings: &HashMap<String, Expr>,
    ) -> bool {
        match directive {
            TransformDirective::Inject(inject) => {
                let expanded = self.substitute_params(&inject.body, param_bindings);
                block.body.extend(expanded);
                true
            }
            TransformDirective::Set(set_block) => {
                for attr in &set_block.attrs {
                    let substituted_value = self.substitute_expr(&attr.value, param_bindings);
                    // Find existing attribute and update, or append
                    let attr_name = &attr.name.name;
                    let mut found = false;
                    for item in &mut block.body {
                        if let BodyItem::Attribute(existing) = item {
                            if existing.name.name == *attr_name {
                                existing.value = substituted_value.clone();
                                found = true;
                                break;
                            }
                        }
                    }
                    if !found {
                        let mut new_attr = attr.clone();
                        new_attr.value = substituted_value;
                        block.body.push(BodyItem::Attribute(new_attr));
                    }
                }
                true
            }
            TransformDirective::Remove(remove_block) => {
                let mut changed = false;
                for target in &remove_block.targets {
                    changed |= self.apply_remove_target(block, target);
                }
                changed
            }
            TransformDirective::Update(update_block) => {
                self.apply_update(block, update_block, param_bindings)
            }
            TransformDirective::When(when_block) => {
                match self.eval_when_condition(&when_block.condition, block, param_bindings) {
                    Some(Value::Bool(true)) => {
                        let mut changed = false;
                        for inner_directive in &when_block.directives {
                            changed |= self.apply_directive(block, inner_directive, param_bindings);
                        }
                        changed
                    }
                    Some(Value::Bool(false)) => false,
                    Some(_) => {
                        self.diagnostics.warning(
                            "when condition evaluated to a non-boolean value; skipping directives"
                                .to_string(),
                            when_block.span,
                        );
                        false
                    }
                    None => {
                        self.diagnostics.warning(
                            "when condition could not be evaluated at macro expansion time; skipping directives"
                                .to_string(),
                            when_block.span,
                        );
                        false
                    }
                }
            }
        }
    }

    /// Check if an InlineId matches a target string.
    fn matches_inline_id(inline_id: &Option<InlineId>, target: &str) -> bool {
        match inline_id {
            Some(InlineId::Literal(lit)) => lit.value == target,
            _ => false,
        }
    }

    /// Apply a single remove target to a block's body.
    fn apply_remove_target(&mut self, block: &mut Block, target: &RemoveTarget) -> bool {
        match target {
            RemoveTarget::Attr(ident) => {
                let name = &ident.name;
                let before = block.body.len();
                block
                    .body
                    .retain(|item| !matches!(item, BodyItem::Attribute(a) if a.name.name == *name));
                block.body.len() != before
            }
            RemoveTarget::Block(kind, id) => {
                let kind_name = &kind.name;
                let id_value = &id.value;
                let before = block.body.len();
                block.body.retain(|item| {
                    if let BodyItem::Block(b) = item {
                        !(b.kind.name == *kind_name
                            && Self::matches_inline_id(&b.inline_id, id_value))
                    } else {
                        true
                    }
                });
                block.body.len() != before
            }
            RemoveTarget::BlockAll(kind) => {
                let kind_name = &kind.name;
                let before = block.body.len();
                block.body.retain(
                    |item| !matches!(item, BodyItem::Block(b) if b.kind.name == *kind_name),
                );
                block.body.len() != before
            }
            RemoveTarget::BlockIndex(kind, n, _) => {
                let kind_name = &kind.name;
                let mut count = 0usize;
                let mut idx_to_remove = None;
                for (i, item) in block.body.iter().enumerate() {
                    if let BodyItem::Block(b) = item {
                        if b.kind.name == *kind_name {
                            if count == *n {
                                idx_to_remove = Some(i);
                                break;
                            }
                            count += 1;
                        }
                    }
                }
                if let Some(idx) = idx_to_remove {
                    block.body.remove(idx);
                    true
                } else {
                    false
                }
            }
            RemoveTarget::Table(id) => {
                let id_value = &id.value;
                let before = block.body.len();
                block.body.retain(|item| {
                    if let BodyItem::Table(t) = item {
                        !Self::matches_inline_id(&t.inline_id, id_value)
                    } else {
                        true
                    }
                });
                block.body.len() != before
            }
            RemoveTarget::AllTables(_) => {
                let before = block.body.len();
                block
                    .body
                    .retain(|item| !matches!(item, BodyItem::Table(_)));
                block.body.len() != before
            }
            RemoveTarget::TableIndex(n, _) => {
                let mut count = 0usize;
                let mut idx_to_remove = None;
                for (i, item) in block.body.iter().enumerate() {
                    if matches!(item, BodyItem::Table(_)) {
                        if count == *n {
                            idx_to_remove = Some(i);
                            break;
                        }
                        count += 1;
                    }
                }
                if let Some(idx) = idx_to_remove {
                    block.body.remove(idx);
                    true
                } else {
                    false
                }
            }
        }
    }

    /// Apply an update directive to a block.
    fn apply_update(
        &mut self,
        block: &mut Block,
        update: &UpdateBlock,
        param_bindings: &HashMap<String, Expr>,
    ) -> bool {
        match &update.selector {
            TargetSelector::BlockKind(kind) => {
                let kind_name = kind.name.clone();
                let mut changed = false;
                for item in &mut block.body {
                    if let BodyItem::Block(child) = item {
                        if child.kind.name == kind_name {
                            for d in &update.block_directives {
                                changed |= self.apply_directive(child, d, param_bindings);
                            }
                        }
                    }
                }
                changed
            }
            TargetSelector::BlockKindId(kind, id) => {
                let kind_name = kind.name.clone();
                let id_value = id.value.clone();
                let mut changed = false;
                for item in &mut block.body {
                    if let BodyItem::Block(child) = item {
                        if child.kind.name == kind_name
                            && Self::matches_inline_id(&child.inline_id, &id_value)
                        {
                            for d in &update.block_directives {
                                changed |= self.apply_directive(child, d, param_bindings);
                            }
                        }
                    }
                }
                changed
            }
            TargetSelector::BlockIndex(kind, n, _) => {
                let kind_name = kind.name.clone();
                let mut count = 0usize;
                let mut changed = false;
                for item in &mut block.body {
                    if let BodyItem::Block(child) = item {
                        if child.kind.name == kind_name {
                            if count == *n {
                                for d in &update.block_directives {
                                    changed |= self.apply_directive(child, d, param_bindings);
                                }
                                break;
                            }
                            count += 1;
                        }
                    }
                }
                changed
            }
            TargetSelector::TableId(id) => {
                let id_value = id.value.clone();
                let mut changed = false;
                for item in &mut block.body {
                    if let BodyItem::Table(table) = item {
                        if Self::matches_inline_id(&table.inline_id, &id_value) {
                            changed |= self.apply_table_directives(
                                table,
                                &update.table_directives,
                                param_bindings,
                            );
                        }
                    }
                }
                changed
            }
            TargetSelector::TableIndex(n, _) => {
                let mut count = 0usize;
                let mut changed = false;
                for item in &mut block.body {
                    if let BodyItem::Table(table) = item {
                        if count == *n {
                            changed |= self.apply_table_directives(
                                table,
                                &update.table_directives,
                                param_bindings,
                            );
                            break;
                        }
                        count += 1;
                    }
                }
                changed
            }
        }
    }

    /// Apply table directives to a table.
    fn apply_table_directives(
        &self,
        table: &mut Table,
        directives: &[TableDirective],
        param_bindings: &HashMap<String, Expr>,
    ) -> bool {
        let mut changed = false;
        for directive in directives {
            match directive {
                TableDirective::InjectRows(rows, _) => {
                    table.rows.extend(rows.iter().cloned());
                    changed = true;
                }
                TableDirective::ClearRows(_) => {
                    if !table.rows.is_empty() {
                        table.rows.clear();
                        changed = true;
                    }
                }
                TableDirective::RemoveRows { condition, .. } => {
                    let before = table.rows.len();
                    let columns = &table.columns;
                    table.rows.retain(|row| {
                        !self.eval_row_condition(condition, row, columns, param_bindings)
                    });
                    if table.rows.len() != before {
                        changed = true;
                    }
                }
                TableDirective::UpdateRows {
                    condition, attrs, ..
                } => {
                    let columns = &table.columns;
                    for row in &mut table.rows {
                        if self.eval_row_condition(condition, row, columns, param_bindings) {
                            // Apply set attrs by column index
                            for (attr_name, attr_val) in attrs {
                                if let Some(col_idx) =
                                    columns.iter().position(|c| c.name.name == attr_name.name)
                                {
                                    if col_idx < row.cells.len() {
                                        row.cells[col_idx] = attr_val.clone();
                                        changed = true;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        changed
    }

    /// Evaluate a row condition by binding column names to cell values.
    fn eval_row_condition(
        &self,
        condition: &Expr,
        row: &TableRow,
        columns: &[ColumnDecl],
        param_bindings: &HashMap<String, Expr>,
    ) -> bool {
        // Build a temporary bindings map: column_name -> cell_expr
        let mut row_bindings = param_bindings.clone();
        for (i, col) in columns.iter().enumerate() {
            if i < row.cells.len() {
                row_bindings.insert(col.name.name.clone(), row.cells[i].clone());
            }
        }
        // Create a dummy block for eval_when_condition
        let dummy_block = Block {
            decorators: vec![],
            partial: false,
            kind: Ident {
                name: "_".to_string(),
                span: Span::dummy(),
            },
            inline_id: None,
            labels: vec![],
            body: vec![],
            text_content: None,
            trivia: wcl_core::trivia::Trivia::empty(),
            span: Span::dummy(),
        };
        match self.eval_when_condition(condition, &dummy_block, &row_bindings) {
            Some(Value::Bool(b)) => b,
            _ => false,
        }
    }

    /// Evaluate a `when` directive condition at macro expansion time.
    ///
    /// Only a limited subset of expressions can be evaluated during Phase 4
    /// (macro expansion). This helper handles:
    /// - Boolean literals (`true`, `false`)
    /// - `self.has("attr")` — checks if the block has an attribute with that name
    /// - `!expr` — logical negation
    /// - `expr && expr` — logical AND
    /// - `expr || expr` — logical OR
    /// - `expr == expr`, `expr != expr` — equality/inequality of evaluated values
    /// - Identifier references that are bound in `param_bindings`
    /// - String/Int/Float/Null literals
    ///
    /// Returns `None` if the expression cannot be evaluated at this phase.
    fn eval_when_condition(
        &self,
        expr: &Expr,
        block: &Block,
        param_bindings: &HashMap<String, Expr>,
    ) -> Option<Value> {
        match expr {
            Expr::BoolLit(b, _) => Some(Value::Bool(*b)),
            Expr::IntLit(i, _) => Some(Value::Int(*i)),
            Expr::FloatLit(f, _) => Some(Value::Float(*f)),
            Expr::NullLit(_) => Some(Value::Null),
            Expr::StringLit(s) => {
                // Only handle non-interpolated strings
                if s.parts.len() == 1 {
                    if let StringPart::Literal(text) = &s.parts[0] {
                        return Some(Value::String(text.clone()));
                    }
                }
                None
            }
            Expr::List(items, _) => {
                let values: Option<Vec<Value>> = items
                    .iter()
                    .map(|item| self.eval_when_condition(item, block, param_bindings))
                    .collect();
                values.map(Value::List)
            }
            Expr::Ident(ident) => {
                // Look up in param_bindings, then try to evaluate the bound expression
                if let Some(bound_expr) = param_bindings.get(&ident.name) {
                    self.eval_when_condition(bound_expr, block, param_bindings)
                } else {
                    None
                }
            }
            Expr::Paren(inner, _) => self.eval_when_condition(inner, block, param_bindings),
            Expr::UnaryOp(UnaryOp::Not, inner, _) => {
                let val = self.eval_when_condition(inner, block, param_bindings)?;
                match val {
                    Value::Bool(b) => Some(Value::Bool(!b)),
                    _ => None,
                }
            }
            Expr::UnaryOp(UnaryOp::Neg, inner, _) => {
                let val = self.eval_when_condition(inner, block, param_bindings)?;
                match val {
                    Value::Int(i) => Some(Value::Int(-i)),
                    Value::Float(f) => Some(Value::Float(-f)),
                    _ => None,
                }
            }
            Expr::BinaryOp(lhs, BinOp::And, rhs, _) => {
                let l = self.eval_when_condition(lhs, block, param_bindings)?;
                match l {
                    Value::Bool(false) => Some(Value::Bool(false)),
                    Value::Bool(true) => {
                        let r = self.eval_when_condition(rhs, block, param_bindings)?;
                        match r {
                            Value::Bool(b) => Some(Value::Bool(b)),
                            _ => None,
                        }
                    }
                    _ => None,
                }
            }
            Expr::BinaryOp(lhs, BinOp::Or, rhs, _) => {
                let l = self.eval_when_condition(lhs, block, param_bindings)?;
                match l {
                    Value::Bool(true) => Some(Value::Bool(true)),
                    Value::Bool(false) => {
                        let r = self.eval_when_condition(rhs, block, param_bindings)?;
                        match r {
                            Value::Bool(b) => Some(Value::Bool(b)),
                            _ => None,
                        }
                    }
                    _ => None,
                }
            }
            Expr::BinaryOp(lhs, BinOp::Eq, rhs, _) => {
                let l = self.eval_when_condition(lhs, block, param_bindings)?;
                let r = self.eval_when_condition(rhs, block, param_bindings)?;
                Some(Value::Bool(l == r))
            }
            Expr::BinaryOp(lhs, BinOp::Neq, rhs, _) => {
                let l = self.eval_when_condition(lhs, block, param_bindings)?;
                let r = self.eval_when_condition(rhs, block, param_bindings)?;
                Some(Value::Bool(l != r))
            }
            // self.field — field access on the target block
            Expr::MemberAccess(obj, field, _) => {
                if let Expr::Ident(ident) = obj.as_ref() {
                    if ident.name == "self" {
                        return self.eval_self_field_access(&field.name, block);
                    }
                }
                None
            }
            // self.has("attr_name") — check if the block has an attribute
            Expr::FnCall(callee, args, _) => {
                if let Expr::MemberAccess(obj, method, _) = callee.as_ref() {
                    if let Expr::Ident(ident) = obj.as_ref() {
                        if ident.name == "self" {
                            return self.eval_self_method_call(
                                &method.name,
                                args,
                                block,
                                param_bindings,
                            );
                        }
                    }
                }
                None
            }
            _ => None,
        }
    }

    /// Evaluate a method call on `self` within a `when` condition.
    ///
    /// Supports:
    /// - `self.has("attr_name")` — returns `Bool(true)` if the block body
    ///   contains an attribute with that name.
    /// - `self.attr("attr_name")` — returns the literal value of the attribute
    ///   if it can be evaluated at macro expansion time.
    fn eval_self_method_call(
        &self,
        method: &str,
        args: &[CallArg],
        block: &Block,
        param_bindings: &HashMap<String, Expr>,
    ) -> Option<Value> {
        match method {
            "has" => {
                // Expect one positional string argument
                if args.len() != 1 {
                    return None;
                }
                let attr_name = match &args[0] {
                    CallArg::Positional(expr) => self.extract_string_arg(expr, param_bindings)?,
                    _ => return None,
                };
                let has_attr = block.body.iter().any(
                    |item| matches!(item, BodyItem::Attribute(attr) if attr.name.name == attr_name),
                );
                Some(Value::Bool(has_attr))
            }
            "attr" => {
                // Expect one positional string argument
                if args.len() != 1 {
                    return None;
                }
                let attr_name = match &args[0] {
                    CallArg::Positional(expr) => self.extract_string_arg(expr, param_bindings)?,
                    _ => return None,
                };
                // Find the attribute and try to evaluate its value as a literal
                for item in &block.body {
                    if let BodyItem::Attribute(attr) = item {
                        if attr.name.name == attr_name {
                            return self.eval_when_condition(&attr.value, block, param_bindings);
                        }
                    }
                }
                None
            }
            _ => None,
        }
    }

    /// Evaluate a field access on `self` within a `when` condition.
    ///
    /// Supports:
    /// - `self.kind` — returns the block kind as a `String`.
    /// - `self.id` — returns the block inline ID as a `String`, or `Null` if absent.
    /// - `self.name` — alias for `self.id`.
    /// - `self.labels` — returns a `List(String)` of the block's labels.
    /// - `self.decorators` — returns a `List(String)` of the block's decorator names.
    fn eval_self_field_access(&self, field: &str, block: &Block) -> Option<Value> {
        match field {
            "kind" => Some(Value::String(block.kind.name.clone())),
            "name" | "id" => {
                match &block.inline_id {
                    Some(InlineId::Literal(id_lit)) => Some(Value::String(id_lit.value.clone())),
                    Some(InlineId::Interpolated(parts)) => {
                        // Concatenate literal parts; interpolations can't be resolved here
                        let s: String = parts
                            .iter()
                            .filter_map(|p| {
                                if let StringPart::Literal(text) = p {
                                    Some(text.as_str())
                                } else {
                                    None
                                }
                            })
                            .collect();
                        Some(Value::String(s))
                    }
                    None => Some(Value::Null),
                }
            }
            "labels" => {
                let label_values: Vec<Value> = block
                    .labels
                    .iter()
                    .map(|label| {
                        let s: String = label
                            .parts
                            .iter()
                            .filter_map(|p| {
                                if let StringPart::Literal(text) = p {
                                    Some(text.as_str())
                                } else {
                                    None
                                }
                            })
                            .collect();
                        Value::String(s)
                    })
                    .collect();
                Some(Value::List(label_values))
            }
            "decorators" => {
                let decorator_names: Vec<Value> = block
                    .decorators
                    .iter()
                    .map(|d| Value::String(d.name.name.clone()))
                    .collect();
                Some(Value::List(decorator_names))
            }
            _ => None,
        }
    }

    /// Extract a string value from an expression (for use as argument to self.has/self.attr).
    fn extract_string_arg(
        &self,
        expr: &Expr,
        param_bindings: &HashMap<String, Expr>,
    ) -> Option<String> {
        match expr {
            Expr::StringLit(s) => {
                if s.parts.len() == 1 {
                    if let StringPart::Literal(text) = &s.parts[0] {
                        return Some(text.clone());
                    }
                }
                None
            }
            Expr::Ident(ident) => {
                if let Some(bound) = param_bindings.get(&ident.name) {
                    self.extract_string_arg(bound, param_bindings)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Bind call arguments to macro parameters, producing a name->Expr map.
    fn bind_params(
        &mut self,
        params: &[MacroParam],
        args: &[MacroCallArg],
        call_span: Span,
    ) -> Result<HashMap<String, Expr>, ()> {
        let mut bindings: HashMap<String, Expr> = HashMap::new();

        // First pass: collect named args
        let mut named_args: HashMap<String, Expr> = HashMap::new();
        let mut positional_args: Vec<Expr> = Vec::new();
        for arg in args {
            match arg {
                MacroCallArg::Positional(expr) => {
                    positional_args.push(expr.clone());
                }
                MacroCallArg::Named(ident, expr) => {
                    named_args.insert(ident.name.clone(), expr.clone());
                }
            }
        }

        // Bind parameters
        let mut pos_idx = 0;
        for param in params {
            let param_name = &param.name.name;

            if let Some(expr) = named_args.remove(param_name) {
                bindings.insert(param_name.clone(), expr);
            } else if pos_idx < positional_args.len() {
                bindings.insert(param_name.clone(), positional_args[pos_idx].clone());
                pos_idx += 1;
            } else if let Some(default) = &param.default {
                bindings.insert(param_name.clone(), default.clone());
            } else {
                self.diagnostics.error(
                    format!("missing required macro parameter '{}'", param_name),
                    call_span,
                );
                return Err(());
            }
        }

        // Check for extra positional args
        if pos_idx < positional_args.len() {
            self.diagnostics.error(
                format!(
                    "too many positional arguments: expected {}, got {}",
                    params.len(),
                    positional_args.len()
                ),
                call_span,
            );
            return Err(());
        }

        // Check for unknown named args
        if let Some(name) = named_args.keys().next() {
            self.diagnostics
                .error(format!("unknown macro parameter: '{}'", name), call_span);
            return Err(());
        }

        // Type-check bound arguments against declared type constraints (E024)
        for param in params {
            if let Some(type_expr) = &param.type_constraint {
                let param_name = &param.name.name;
                if let Some(bound_expr) = bindings.get(param_name) {
                    if let Some(value) = Self::expr_to_literal_value(bound_expr) {
                        if !Self::value_matches_type(&value, type_expr) {
                            self.diagnostics.error(
                                format!(
                                    "E024: macro parameter '{}' expects type {}, got {}",
                                    param_name,
                                    Self::type_expr_display(type_expr),
                                    Self::value_type_name(&value),
                                ),
                                call_span,
                            );
                            return Err(());
                        }
                    }
                    // If we can't evaluate the expression to a literal, skip the check
                }
            }
        }

        Ok(bindings)
    }

    /// Try to evaluate a literal expression to a Value at macro expansion time.
    fn expr_to_literal_value(expr: &Expr) -> Option<Value> {
        match expr {
            Expr::BoolLit(b, _) => Some(Value::Bool(*b)),
            Expr::IntLit(i, _) => Some(Value::Int(*i)),
            Expr::FloatLit(f, _) => Some(Value::Float(*f)),
            Expr::NullLit(_) => Some(Value::Null),
            Expr::StringLit(s) => {
                let text: String = s
                    .parts
                    .iter()
                    .filter_map(|p| {
                        if let StringPart::Literal(t) = p {
                            Some(t.as_str())
                        } else {
                            None
                        }
                    })
                    .collect();
                Some(Value::String(text))
            }
            Expr::List(items, _) => {
                let values: Option<Vec<Value>> =
                    items.iter().map(Self::expr_to_literal_value).collect();
                values.map(Value::List)
            }
            _ => None,
        }
    }

    /// Check if a Value matches a TypeExpr.
    fn value_matches_type(value: &Value, type_expr: &TypeExpr) -> bool {
        match type_expr {
            TypeExpr::String(_) => matches!(value, Value::String(_)),
            TypeExpr::Int(_) => matches!(value, Value::Int(_)),
            TypeExpr::Float(_) => matches!(value, Value::Float(_) | Value::Int(_)),
            TypeExpr::Bool(_) => matches!(value, Value::Bool(_)),
            TypeExpr::Null(_) => matches!(value, Value::Null),
            TypeExpr::Any(_) => true,
            TypeExpr::List(inner_type, _) => {
                if let Value::List(items) = value {
                    items
                        .iter()
                        .all(|item| Self::value_matches_type(item, inner_type))
                } else {
                    false
                }
            }
            TypeExpr::Union(types, _) => types.iter().any(|t| Self::value_matches_type(value, t)),
            // For types we can't check at macro expansion time, pass through
            _ => true,
        }
    }

    /// Human-readable name for a TypeExpr.
    fn type_expr_display(type_expr: &TypeExpr) -> String {
        match type_expr {
            TypeExpr::String(_) => "string".to_string(),
            TypeExpr::Int(_) => "int".to_string(),
            TypeExpr::Float(_) => "float".to_string(),
            TypeExpr::Bool(_) => "bool".to_string(),
            TypeExpr::Null(_) => "null".to_string(),
            TypeExpr::Any(_) => "any".to_string(),
            TypeExpr::Identifier(_) => "identifier".to_string(),
            TypeExpr::List(inner, _) => format!("list({})", Self::type_expr_display(inner)),
            TypeExpr::Map(k, v, _) => format!(
                "map({}, {})",
                Self::type_expr_display(k),
                Self::type_expr_display(v)
            ),
            TypeExpr::Set(inner, _) => format!("set({})", Self::type_expr_display(inner)),
            TypeExpr::Ref(_, _) => "ref(...)".to_string(),
            TypeExpr::Union(types, _) => {
                let parts: Vec<String> = types.iter().map(Self::type_expr_display).collect();
                format!("union({})", parts.join(", "))
            }
        }
    }

    /// Human-readable name for a Value's runtime type.
    fn value_type_name(value: &Value) -> String {
        match value {
            Value::String(_) => "string".to_string(),
            Value::Int(_) => "int".to_string(),
            Value::Float(_) => "float".to_string(),
            Value::Bool(_) => "bool".to_string(),
            Value::Null => "null".to_string(),
            Value::Identifier(_) => "identifier".to_string(),
            Value::List(_) => "list".to_string(),
            Value::Map(_) => "map".to_string(),
            Value::Set(_) => "set".to_string(),
            Value::BlockRef(_) => "block".to_string(),
            Value::Function(_) => "function".to_string(),
        }
    }

    /// Substitute macro parameters in a list of body items.
    ///
    /// Deep clones the body items, replacing `Ident` references that match
    /// parameter names with the corresponding expression.
    fn substitute_params(
        &self,
        body: &[BodyItem],
        params: &HashMap<String, Expr>,
    ) -> Vec<BodyItem> {
        body.iter()
            .map(|item| self.substitute_body_item(item, params))
            .collect()
    }

    fn substitute_body_item(&self, item: &BodyItem, params: &HashMap<String, Expr>) -> BodyItem {
        match item {
            BodyItem::Attribute(attr) => {
                let mut new_attr = attr.clone();
                new_attr.value = self.substitute_expr(&attr.value, params);
                BodyItem::Attribute(new_attr)
            }
            BodyItem::Block(block) => {
                let mut new_block = block.clone();
                new_block.body = self.substitute_params(&block.body, params);
                // Also substitute in labels
                new_block.labels = block
                    .labels
                    .iter()
                    .map(|l| self.substitute_string_lit(l, params))
                    .collect();
                // Substitute in text content
                if let Some(ref tc) = block.text_content {
                    new_block.text_content = Some(self.substitute_string_lit(tc, params));
                }
                BodyItem::Block(new_block)
            }
            BodyItem::LetBinding(lb) => {
                let mut new_lb = lb.clone();
                new_lb.value = self.substitute_expr(&lb.value, params);
                BodyItem::LetBinding(new_lb)
            }
            BodyItem::MacroCall(call) => {
                let mut new_call = call.clone();
                new_call.args = call
                    .args
                    .iter()
                    .map(|arg| match arg {
                        MacroCallArg::Positional(expr) => {
                            MacroCallArg::Positional(self.substitute_expr(expr, params))
                        }
                        MacroCallArg::Named(ident, expr) => {
                            MacroCallArg::Named(ident.clone(), self.substitute_expr(expr, params))
                        }
                    })
                    .collect();
                BodyItem::MacroCall(new_call)
            }
            BodyItem::ForLoop(fl) => {
                let mut new_fl = fl.clone();
                new_fl.iterable = self.substitute_expr(&fl.iterable, params);
                new_fl.body = self.substitute_params(&fl.body, params);
                BodyItem::ForLoop(new_fl)
            }
            BodyItem::Conditional(cond) => {
                let mut new_cond = cond.clone();
                new_cond.condition = self.substitute_expr(&cond.condition, params);
                new_cond.then_body = self.substitute_params(&cond.then_body, params);
                if let Some(else_branch) = &cond.else_branch {
                    new_cond.else_branch = Some(self.substitute_else_branch(else_branch, params));
                }
                BodyItem::Conditional(new_cond)
            }
            // Items that don't contain parameter references
            other => other.clone(),
        }
    }

    fn substitute_else_branch(
        &self,
        branch: &ElseBranch,
        params: &HashMap<String, Expr>,
    ) -> ElseBranch {
        match branch {
            ElseBranch::ElseIf(cond) => {
                let mut new_cond = (**cond).clone();
                new_cond.condition = self.substitute_expr(&cond.condition, params);
                new_cond.then_body = self.substitute_params(&cond.then_body, params);
                if let Some(else_branch) = &cond.else_branch {
                    new_cond.else_branch = Some(self.substitute_else_branch(else_branch, params));
                }
                ElseBranch::ElseIf(Box::new(new_cond))
            }
            ElseBranch::Else(body, trivia, span) => {
                ElseBranch::Else(self.substitute_params(body, params), trivia.clone(), *span)
            }
        }
    }

    /// Substitute parameter references within an expression.
    fn substitute_expr(&self, expr: &Expr, params: &HashMap<String, Expr>) -> Expr {
        match expr {
            Expr::Ident(ident) => {
                if let Some(replacement) = params.get(&ident.name) {
                    replacement.clone()
                } else {
                    expr.clone()
                }
            }
            Expr::BinaryOp(lhs, op, rhs, span) => Expr::BinaryOp(
                Box::new(self.substitute_expr(lhs, params)),
                *op,
                Box::new(self.substitute_expr(rhs, params)),
                *span,
            ),
            Expr::UnaryOp(op, operand, span) => {
                Expr::UnaryOp(*op, Box::new(self.substitute_expr(operand, params)), *span)
            }
            Expr::Ternary(cond, then_expr, else_expr, span) => Expr::Ternary(
                Box::new(self.substitute_expr(cond, params)),
                Box::new(self.substitute_expr(then_expr, params)),
                Box::new(self.substitute_expr(else_expr, params)),
                *span,
            ),
            Expr::MemberAccess(obj, field, span) => Expr::MemberAccess(
                Box::new(self.substitute_expr(obj, params)),
                field.clone(),
                *span,
            ),
            Expr::IndexAccess(obj, idx, span) => Expr::IndexAccess(
                Box::new(self.substitute_expr(obj, params)),
                Box::new(self.substitute_expr(idx, params)),
                *span,
            ),
            Expr::FnCall(callee, args, span) => {
                let new_args = args
                    .iter()
                    .map(|arg| match arg {
                        CallArg::Positional(e) => {
                            CallArg::Positional(self.substitute_expr(e, params))
                        }
                        CallArg::Named(ident, e) => {
                            CallArg::Named(ident.clone(), self.substitute_expr(e, params))
                        }
                    })
                    .collect();
                Expr::FnCall(
                    Box::new(self.substitute_expr(callee, params)),
                    new_args,
                    *span,
                )
            }
            Expr::List(items, span) => {
                let new_items = items
                    .iter()
                    .map(|e| self.substitute_expr(e, params))
                    .collect();
                Expr::List(new_items, *span)
            }
            Expr::Map(entries, span) => {
                let new_entries = entries
                    .iter()
                    .map(|(k, v)| (k.clone(), self.substitute_expr(v, params)))
                    .collect();
                Expr::Map(new_entries, *span)
            }
            Expr::StringLit(string_lit) => {
                Expr::StringLit(self.substitute_string_lit(string_lit, params))
            }
            Expr::Paren(inner, span) => {
                Expr::Paren(Box::new(self.substitute_expr(inner, params)), *span)
            }
            Expr::Lambda(idents, body, span) => {
                // Don't substitute params that are shadowed by lambda params
                let lambda_param_names: Vec<&str> =
                    idents.iter().map(|i| i.name.as_str()).collect();
                let filtered_params: HashMap<String, Expr> = params
                    .iter()
                    .filter(|(k, _)| !lambda_param_names.contains(&k.as_str()))
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                Expr::Lambda(
                    idents.clone(),
                    Box::new(self.substitute_expr(body, &filtered_params)),
                    *span,
                )
            }
            // Literals and other expressions that don't contain ident references
            _ => expr.clone(),
        }
    }

    /// Substitute parameter references within a string literal's interpolations.
    fn substitute_string_lit(&self, lit: &StringLit, params: &HashMap<String, Expr>) -> StringLit {
        StringLit {
            parts: lit
                .parts
                .iter()
                .map(|part| match part {
                    StringPart::Literal(s) => StringPart::Literal(s.clone()),
                    StringPart::Interpolation(expr) => {
                        StringPart::Interpolation(Box::new(self.substitute_expr(expr, params)))
                    }
                })
                .collect(),
            span: lit.span,
        }
    }

    /// Consume the expander and return accumulated diagnostics.
    pub fn into_diagnostics(self) -> DiagnosticBag {
        self.diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wcl_core::span::{FileId, Span};
    use wcl_core::trivia::Trivia;

    fn dummy_span() -> Span {
        Span::new(FileId(0), 0, 0)
    }

    fn make_ident(name: &str) -> Ident {
        Ident {
            name: name.to_string(),
            span: dummy_span(),
        }
    }

    fn make_simple_macro_def(name: &str, params: Vec<&str>) -> MacroDef {
        MacroDef {
            decorators: vec![],
            kind: MacroKind::Function,
            name: make_ident(name),
            params: params
                .into_iter()
                .map(|p| MacroParam {
                    name: make_ident(p),
                    type_constraint: None,
                    default: None,
                    span: dummy_span(),
                })
                .collect(),
            body: MacroBody::Function(vec![BodyItem::Attribute(Attribute {
                decorators: vec![],
                name: make_ident("generated"),
                value: Expr::BoolLit(true, dummy_span()),
                trivia: Trivia::empty(),
                span: dummy_span(),
            })]),
            trivia: Trivia::empty(),
            span: dummy_span(),
        }
    }

    fn make_attr_macro_def(name: &str) -> MacroDef {
        MacroDef {
            decorators: vec![],
            kind: MacroKind::Attribute,
            name: make_ident(name),
            params: vec![],
            body: MacroBody::Attribute(vec![TransformDirective::Set(SetBlock {
                attrs: vec![Attribute {
                    decorators: vec![],
                    name: make_ident("injected"),
                    value: Expr::BoolLit(true, dummy_span()),
                    trivia: Trivia::empty(),
                    span: dummy_span(),
                }],
                span: dummy_span(),
            })]),
            trivia: Trivia::empty(),
            span: dummy_span(),
        }
    }

    #[test]
    fn collect_registers_function_macros() {
        let mut registry = MacroRegistry::new();
        let mut diags = DiagnosticBag::new();

        let macro_def = make_simple_macro_def("my_macro", vec!["arg1"]);
        let mut doc = Document {
            items: vec![DocItem::Body(BodyItem::MacroDef(macro_def))],
            trivia: Trivia::empty(),
            span: dummy_span(),
        };

        registry.collect(&mut doc, &mut diags);

        assert!(!diags.has_errors());
        assert!(registry.has_function_macro("my_macro"));
        assert!(!registry.has_attribute_macro("my_macro"));
        // MacroDef should be removed from the document
        assert!(doc.items.is_empty());
    }

    #[test]
    fn collect_registers_attribute_macros() {
        let mut registry = MacroRegistry::new();
        let mut diags = DiagnosticBag::new();

        let macro_def = make_attr_macro_def("with_logging");
        let mut doc = Document {
            items: vec![DocItem::Body(BodyItem::MacroDef(macro_def))],
            trivia: Trivia::empty(),
            span: dummy_span(),
        };

        registry.collect(&mut doc, &mut diags);

        assert!(!diags.has_errors());
        assert!(registry.has_attribute_macro("with_logging"));
        assert!(!registry.has_function_macro("with_logging"));
        assert!(doc.items.is_empty());
    }

    #[test]
    fn collect_detects_duplicate_function_macros() {
        let mut registry = MacroRegistry::new();
        let mut diags = DiagnosticBag::new();

        let macro1 = make_simple_macro_def("dup", vec![]);
        let macro2 = make_simple_macro_def("dup", vec![]);
        let mut doc = Document {
            items: vec![
                DocItem::Body(BodyItem::MacroDef(macro1)),
                DocItem::Body(BodyItem::MacroDef(macro2)),
            ],
            trivia: Trivia::empty(),
            span: dummy_span(),
        };

        registry.collect(&mut doc, &mut diags);

        assert!(diags.has_errors());
        assert_eq!(diags.error_count(), 1);
    }

    #[test]
    fn collect_preserves_non_macro_items() {
        let mut registry = MacroRegistry::new();
        let mut diags = DiagnosticBag::new();

        let macro_def = make_simple_macro_def("my_macro", vec![]);
        let attr = BodyItem::Attribute(Attribute {
            decorators: vec![],
            name: make_ident("port"),
            value: Expr::IntLit(8080, dummy_span()),
            trivia: Trivia::empty(),
            span: dummy_span(),
        });

        let mut doc = Document {
            items: vec![
                DocItem::Body(BodyItem::MacroDef(macro_def)),
                DocItem::Body(attr),
            ],
            trivia: Trivia::empty(),
            span: dummy_span(),
        };

        registry.collect(&mut doc, &mut diags);

        assert!(!diags.has_errors());
        assert_eq!(doc.items.len(), 1); // Only the attribute remains
        assert!(registry.has_function_macro("my_macro"));
    }

    // ── when directive ──────────────────────────────────────────────────────

    fn make_block_with_attrs(attrs: Vec<(&str, Expr)>) -> Block {
        Block {
            decorators: vec![],
            partial: false,
            kind: make_ident("server"),
            inline_id: None,
            labels: vec![],
            body: attrs
                .into_iter()
                .map(|(name, value)| {
                    BodyItem::Attribute(Attribute {
                        decorators: vec![],
                        name: make_ident(name),
                        value,
                        trivia: Trivia::empty(),
                        span: dummy_span(),
                    })
                })
                .collect(),
            text_content: None,
            trivia: Trivia::empty(),
            span: dummy_span(),
        }
    }

    fn make_when_attr_macro(
        name: &str,
        condition: Expr,
        inner_directives: Vec<TransformDirective>,
    ) -> MacroDef {
        MacroDef {
            decorators: vec![],
            kind: MacroKind::Attribute,
            name: make_ident(name),
            params: vec![],
            body: MacroBody::Attribute(vec![TransformDirective::When(WhenBlock {
                condition,
                directives: inner_directives,
                span: dummy_span(),
            })]),
            trivia: Trivia::empty(),
            span: dummy_span(),
        }
    }

    #[test]
    fn when_true_literal_applies_inner_directives() {
        let mut registry = MacroRegistry::new();
        let inner_set = TransformDirective::Set(SetBlock {
            attrs: vec![Attribute {
                decorators: vec![],
                name: make_ident("added"),
                value: Expr::BoolLit(true, dummy_span()),
                trivia: Trivia::empty(),
                span: dummy_span(),
            }],
            span: dummy_span(),
        });
        let macro_def = make_when_attr_macro(
            "add_if_true",
            Expr::BoolLit(true, dummy_span()),
            vec![inner_set],
        );
        registry
            .attribute_macros
            .insert("add_if_true".to_string(), macro_def);

        let mut block = make_block_with_attrs(vec![("port", Expr::IntLit(8080, dummy_span()))]);
        block.decorators.push(Decorator {
            name: make_ident("add_if_true"),
            args: vec![],
            span: dummy_span(),
        });

        let mut expander = MacroExpander::new(&registry, 10);
        let changed = expander.apply_attribute_macros(&mut block);

        assert!(changed);
        // Should have original "port" + new "added"
        assert_eq!(block.body.len(), 2);
        assert!(block.body.iter().any(|item| {
            matches!(item, BodyItem::Attribute(attr) if attr.name.name == "added")
        }));
    }

    #[test]
    fn when_false_literal_skips_inner_directives() {
        let mut registry = MacroRegistry::new();
        let inner_set = TransformDirective::Set(SetBlock {
            attrs: vec![Attribute {
                decorators: vec![],
                name: make_ident("added"),
                value: Expr::BoolLit(true, dummy_span()),
                trivia: Trivia::empty(),
                span: dummy_span(),
            }],
            span: dummy_span(),
        });
        let macro_def = make_when_attr_macro(
            "add_if_false",
            Expr::BoolLit(false, dummy_span()),
            vec![inner_set],
        );
        registry
            .attribute_macros
            .insert("add_if_false".to_string(), macro_def);

        let mut block = make_block_with_attrs(vec![("port", Expr::IntLit(8080, dummy_span()))]);
        block.decorators.push(Decorator {
            name: make_ident("add_if_false"),
            args: vec![],
            span: dummy_span(),
        });

        let mut expander = MacroExpander::new(&registry, 10);
        let changed = expander.apply_attribute_macros(&mut block);

        // when(false) returns false for changed, but the decorator was still consumed
        // The block body should only have "port"
        assert_eq!(block.body.len(), 1);
        assert!(!block.body.iter().any(|item| {
            matches!(item, BodyItem::Attribute(attr) if attr.name.name == "added")
        }));
        let _ = changed;
    }

    #[test]
    fn when_self_has_present_attribute_applies_directives() {
        let mut registry = MacroRegistry::new();
        // Condition: self.has("port")
        let condition = Expr::FnCall(
            Box::new(Expr::MemberAccess(
                Box::new(Expr::Ident(make_ident("self"))),
                make_ident("has"),
                dummy_span(),
            )),
            vec![CallArg::Positional(Expr::StringLit(StringLit {
                parts: vec![StringPart::Literal("port".to_string())],
                span: dummy_span(),
            }))],
            dummy_span(),
        );
        let inner_set = TransformDirective::Set(SetBlock {
            attrs: vec![Attribute {
                decorators: vec![],
                name: make_ident("has_port"),
                value: Expr::BoolLit(true, dummy_span()),
                trivia: Trivia::empty(),
                span: dummy_span(),
            }],
            span: dummy_span(),
        });
        let macro_def = make_when_attr_macro("check_port", condition, vec![inner_set]);
        registry
            .attribute_macros
            .insert("check_port".to_string(), macro_def);

        let mut block = make_block_with_attrs(vec![("port", Expr::IntLit(8080, dummy_span()))]);
        block.decorators.push(Decorator {
            name: make_ident("check_port"),
            args: vec![],
            span: dummy_span(),
        });

        let mut expander = MacroExpander::new(&registry, 10);
        let changed = expander.apply_attribute_macros(&mut block);

        assert!(changed);
        assert_eq!(block.body.len(), 2);
        assert!(block.body.iter().any(|item| {
            matches!(item, BodyItem::Attribute(attr) if attr.name.name == "has_port")
        }));
    }

    #[test]
    fn when_self_has_absent_attribute_skips_directives() {
        let mut registry = MacroRegistry::new();
        // Condition: self.has("missing_attr")
        let condition = Expr::FnCall(
            Box::new(Expr::MemberAccess(
                Box::new(Expr::Ident(make_ident("self"))),
                make_ident("has"),
                dummy_span(),
            )),
            vec![CallArg::Positional(Expr::StringLit(StringLit {
                parts: vec![StringPart::Literal("missing_attr".to_string())],
                span: dummy_span(),
            }))],
            dummy_span(),
        );
        let inner_set = TransformDirective::Set(SetBlock {
            attrs: vec![Attribute {
                decorators: vec![],
                name: make_ident("should_not_exist"),
                value: Expr::BoolLit(true, dummy_span()),
                trivia: Trivia::empty(),
                span: dummy_span(),
            }],
            span: dummy_span(),
        });
        let macro_def = make_when_attr_macro("check_missing", condition, vec![inner_set]);
        registry
            .attribute_macros
            .insert("check_missing".to_string(), macro_def);

        let mut block = make_block_with_attrs(vec![("port", Expr::IntLit(8080, dummy_span()))]);
        block.decorators.push(Decorator {
            name: make_ident("check_missing"),
            args: vec![],
            span: dummy_span(),
        });

        let mut expander = MacroExpander::new(&registry, 10);
        expander.apply_attribute_macros(&mut block);

        assert_eq!(block.body.len(), 1);
        assert!(!block.body.iter().any(|item| {
            matches!(item, BodyItem::Attribute(attr) if attr.name.name == "should_not_exist")
        }));
    }

    #[test]
    fn when_negated_condition_works() {
        let mut registry = MacroRegistry::new();
        // Condition: !self.has("port") — block has port, so !true = false
        let condition = Expr::UnaryOp(
            UnaryOp::Not,
            Box::new(Expr::FnCall(
                Box::new(Expr::MemberAccess(
                    Box::new(Expr::Ident(make_ident("self"))),
                    make_ident("has"),
                    dummy_span(),
                )),
                vec![CallArg::Positional(Expr::StringLit(StringLit {
                    parts: vec![StringPart::Literal("port".to_string())],
                    span: dummy_span(),
                }))],
                dummy_span(),
            )),
            dummy_span(),
        );
        let inner_set = TransformDirective::Set(SetBlock {
            attrs: vec![Attribute {
                decorators: vec![],
                name: make_ident("no_port"),
                value: Expr::BoolLit(true, dummy_span()),
                trivia: Trivia::empty(),
                span: dummy_span(),
            }],
            span: dummy_span(),
        });
        let macro_def = make_when_attr_macro("check_no_port", condition, vec![inner_set]);
        registry
            .attribute_macros
            .insert("check_no_port".to_string(), macro_def);

        let mut block = make_block_with_attrs(vec![("port", Expr::IntLit(8080, dummy_span()))]);
        block.decorators.push(Decorator {
            name: make_ident("check_no_port"),
            args: vec![],
            span: dummy_span(),
        });

        let mut expander = MacroExpander::new(&registry, 10);
        expander.apply_attribute_macros(&mut block);

        // !self.has("port") is false because port exists, so inner directives skipped
        assert_eq!(block.body.len(), 1);
        assert!(!block.body.iter().any(|item| {
            matches!(item, BodyItem::Attribute(attr) if attr.name.name == "no_port")
        }));
    }

    #[test]
    fn when_unevaluable_condition_emits_warning_and_skips() {
        let mut registry = MacroRegistry::new();
        // Condition that can't be evaluated: some_var (not in param_bindings, not a literal)
        let condition = Expr::Ident(make_ident("unknown_variable"));
        let inner_set = TransformDirective::Set(SetBlock {
            attrs: vec![Attribute {
                decorators: vec![],
                name: make_ident("should_not_exist"),
                value: Expr::BoolLit(true, dummy_span()),
                trivia: Trivia::empty(),
                span: dummy_span(),
            }],
            span: dummy_span(),
        });
        let macro_def = make_when_attr_macro("check_unknown", condition, vec![inner_set]);
        registry
            .attribute_macros
            .insert("check_unknown".to_string(), macro_def);

        let mut block = make_block_with_attrs(vec![("port", Expr::IntLit(8080, dummy_span()))]);
        block.decorators.push(Decorator {
            name: make_ident("check_unknown"),
            args: vec![],
            span: dummy_span(),
        });

        let mut expander = MacroExpander::new(&registry, 10);
        expander.apply_attribute_macros(&mut block);

        // Should skip and emit a warning
        assert_eq!(block.body.len(), 1);
        let diags = expander.into_diagnostics();
        assert!(!diags.is_empty());
    }

    // ── Gap 7: self.labels and self.decorators field access ──────────────

    #[test]
    fn self_labels_returns_list_of_label_strings() {
        let registry = MacroRegistry::new();
        let expander = MacroExpander::new(&registry, 10);

        let block = Block {
            decorators: vec![],
            partial: false,
            kind: make_ident("server"),
            inline_id: None,
            labels: vec![
                StringLit {
                    parts: vec![StringPart::Literal("web".to_string())],
                    span: dummy_span(),
                },
                StringLit {
                    parts: vec![StringPart::Literal("prod".to_string())],
                    span: dummy_span(),
                },
            ],
            body: vec![],
            text_content: None,
            trivia: Trivia::empty(),
            span: dummy_span(),
        };

        let result = expander.eval_self_field_access("labels", &block);
        assert_eq!(
            result,
            Some(Value::List(vec![
                Value::String("web".to_string()),
                Value::String("prod".to_string()),
            ]))
        );
    }

    #[test]
    fn self_labels_returns_empty_list_when_no_labels() {
        let registry = MacroRegistry::new();
        let expander = MacroExpander::new(&registry, 10);

        let block = make_block_with_attrs(vec![]);
        let result = expander.eval_self_field_access("labels", &block);
        assert_eq!(result, Some(Value::List(vec![])));
    }

    #[test]
    fn self_decorators_returns_list_of_decorator_names() {
        let registry = MacroRegistry::new();
        let expander = MacroExpander::new(&registry, 10);

        let block = Block {
            decorators: vec![
                Decorator {
                    name: make_ident("logging"),
                    args: vec![],
                    span: dummy_span(),
                },
                Decorator {
                    name: make_ident("monitoring"),
                    args: vec![],
                    span: dummy_span(),
                },
            ],
            partial: false,
            kind: make_ident("server"),
            inline_id: None,
            labels: vec![],
            body: vec![],
            text_content: None,
            trivia: Trivia::empty(),
            span: dummy_span(),
        };

        let result = expander.eval_self_field_access("decorators", &block);
        assert_eq!(
            result,
            Some(Value::List(vec![
                Value::String("logging".to_string()),
                Value::String("monitoring".to_string()),
            ]))
        );
    }

    #[test]
    fn self_decorators_returns_empty_list_when_no_decorators() {
        let registry = MacroRegistry::new();
        let expander = MacroExpander::new(&registry, 10);

        let block = make_block_with_attrs(vec![]);
        let result = expander.eval_self_field_access("decorators", &block);
        assert_eq!(result, Some(Value::List(vec![])));
    }

    #[test]
    fn self_kind_returns_block_kind() {
        let registry = MacroRegistry::new();
        let expander = MacroExpander::new(&registry, 10);

        let block = make_block_with_attrs(vec![]);
        let result = expander.eval_self_field_access("kind", &block);
        assert_eq!(result, Some(Value::String("server".to_string())));
    }

    #[test]
    fn self_id_returns_inline_id() {
        let registry = MacroRegistry::new();
        let expander = MacroExpander::new(&registry, 10);

        let mut block = make_block_with_attrs(vec![]);
        block.inline_id = Some(InlineId::Literal(IdentifierLit {
            value: "my-server".to_string(),
            span: dummy_span(),
        }));

        let result = expander.eval_self_field_access("id", &block);
        assert_eq!(result, Some(Value::String("my-server".to_string())));
    }

    #[test]
    fn self_id_returns_null_when_absent() {
        let registry = MacroRegistry::new();
        let expander = MacroExpander::new(&registry, 10);

        let block = make_block_with_attrs(vec![]);
        let result = expander.eval_self_field_access("id", &block);
        assert_eq!(result, Some(Value::Null));
    }

    #[test]
    fn self_labels_in_when_condition() {
        let mut registry = MacroRegistry::new();
        // Condition: self.labels == ["web"]
        let condition = Expr::BinaryOp(
            Box::new(Expr::MemberAccess(
                Box::new(Expr::Ident(make_ident("self"))),
                make_ident("labels"),
                dummy_span(),
            )),
            BinOp::Eq,
            Box::new(Expr::List(
                vec![Expr::StringLit(StringLit {
                    parts: vec![StringPart::Literal("web".to_string())],
                    span: dummy_span(),
                })],
                dummy_span(),
            )),
            dummy_span(),
        );
        let inner_set = TransformDirective::Set(SetBlock {
            attrs: vec![Attribute {
                decorators: vec![],
                name: make_ident("is_web"),
                value: Expr::BoolLit(true, dummy_span()),
                trivia: Trivia::empty(),
                span: dummy_span(),
            }],
            span: dummy_span(),
        });
        let macro_def = make_when_attr_macro("check_labels", condition, vec![inner_set]);
        registry
            .attribute_macros
            .insert("check_labels".to_string(), macro_def);

        let mut block = Block {
            decorators: vec![Decorator {
                name: make_ident("check_labels"),
                args: vec![],
                span: dummy_span(),
            }],
            partial: false,
            kind: make_ident("server"),
            inline_id: None,
            labels: vec![StringLit {
                parts: vec![StringPart::Literal("web".to_string())],
                span: dummy_span(),
            }],
            body: vec![BodyItem::Attribute(Attribute {
                decorators: vec![],
                name: make_ident("port"),
                value: Expr::IntLit(8080, dummy_span()),
                trivia: Trivia::empty(),
                span: dummy_span(),
            })],
            text_content: None,
            trivia: Trivia::empty(),
            span: dummy_span(),
        };

        let mut expander = MacroExpander::new(&registry, 10);
        let changed = expander.apply_attribute_macros(&mut block);

        assert!(changed);
        assert!(block.body.iter().any(|item| {
            matches!(item, BodyItem::Attribute(attr) if attr.name.name == "is_web")
        }));
    }

    // ── Gap 8: Macro parameter type checking ─────────────────────────────

    #[test]
    fn bind_params_type_check_passes_for_matching_type() {
        let registry = MacroRegistry::new();
        let mut expander = MacroExpander::new(&registry, 10);

        let params = vec![MacroParam {
            name: make_ident("port"),
            type_constraint: Some(TypeExpr::Int(dummy_span())),
            default: None,
            span: dummy_span(),
        }];
        let args = vec![MacroCallArg::Positional(Expr::IntLit(8080, dummy_span()))];

        let result = expander.bind_params(&params, &args, dummy_span());
        assert!(result.is_ok());
    }

    #[test]
    fn bind_params_type_check_fails_for_wrong_type() {
        let registry = MacroRegistry::new();
        let mut expander = MacroExpander::new(&registry, 10);

        let params = vec![MacroParam {
            name: make_ident("port"),
            type_constraint: Some(TypeExpr::Int(dummy_span())),
            default: None,
            span: dummy_span(),
        }];
        let args = vec![MacroCallArg::Positional(Expr::StringLit(StringLit {
            parts: vec![StringPart::Literal("not_an_int".to_string())],
            span: dummy_span(),
        }))];

        let result = expander.bind_params(&params, &args, dummy_span());
        assert!(result.is_err());

        let diags = expander.into_diagnostics();
        assert!(diags
            .diagnostics()
            .iter()
            .any(|d| d.message.contains("E024")));
    }

    #[test]
    fn bind_params_type_check_passes_for_bool() {
        let registry = MacroRegistry::new();
        let mut expander = MacroExpander::new(&registry, 10);

        let params = vec![MacroParam {
            name: make_ident("enabled"),
            type_constraint: Some(TypeExpr::Bool(dummy_span())),
            default: None,
            span: dummy_span(),
        }];
        let args = vec![MacroCallArg::Positional(Expr::BoolLit(true, dummy_span()))];

        let result = expander.bind_params(&params, &args, dummy_span());
        assert!(result.is_ok());
    }

    #[test]
    fn bind_params_type_check_string_rejects_int() {
        let registry = MacroRegistry::new();
        let mut expander = MacroExpander::new(&registry, 10);

        let params = vec![MacroParam {
            name: make_ident("name"),
            type_constraint: Some(TypeExpr::String(dummy_span())),
            default: None,
            span: dummy_span(),
        }];
        let args = vec![MacroCallArg::Positional(Expr::IntLit(42, dummy_span()))];

        let result = expander.bind_params(&params, &args, dummy_span());
        assert!(result.is_err());

        let diags = expander.into_diagnostics();
        assert!(diags
            .diagnostics()
            .iter()
            .any(|d| d.message.contains("E024")));
    }

    #[test]
    fn bind_params_no_type_constraint_skips_check() {
        let registry = MacroRegistry::new();
        let mut expander = MacroExpander::new(&registry, 10);

        let params = vec![MacroParam {
            name: make_ident("x"),
            type_constraint: None,
            default: None,
            span: dummy_span(),
        }];
        let args = vec![MacroCallArg::Positional(Expr::IntLit(42, dummy_span()))];

        let result = expander.bind_params(&params, &args, dummy_span());
        assert!(result.is_ok());
    }

    #[test]
    fn bind_params_type_check_list_of_string() {
        let registry = MacroRegistry::new();
        let mut expander = MacroExpander::new(&registry, 10);

        let params = vec![MacroParam {
            name: make_ident("tags"),
            type_constraint: Some(TypeExpr::List(
                Box::new(TypeExpr::String(dummy_span())),
                dummy_span(),
            )),
            default: None,
            span: dummy_span(),
        }];
        let args = vec![MacroCallArg::Positional(Expr::List(
            vec![
                Expr::StringLit(StringLit {
                    parts: vec![StringPart::Literal("a".to_string())],
                    span: dummy_span(),
                }),
                Expr::StringLit(StringLit {
                    parts: vec![StringPart::Literal("b".to_string())],
                    span: dummy_span(),
                }),
            ],
            dummy_span(),
        ))];

        let result = expander.bind_params(&params, &args, dummy_span());
        assert!(result.is_ok());
    }

    #[test]
    fn bind_params_type_check_list_rejects_wrong_inner_type() {
        let registry = MacroRegistry::new();
        let mut expander = MacroExpander::new(&registry, 10);

        let params = vec![MacroParam {
            name: make_ident("tags"),
            type_constraint: Some(TypeExpr::List(
                Box::new(TypeExpr::String(dummy_span())),
                dummy_span(),
            )),
            default: None,
            span: dummy_span(),
        }];
        let args = vec![MacroCallArg::Positional(Expr::List(
            vec![Expr::IntLit(42, dummy_span())],
            dummy_span(),
        ))];

        let result = expander.bind_params(&params, &args, dummy_span());
        assert!(result.is_err());

        let diags = expander.into_diagnostics();
        assert!(diags
            .diagnostics()
            .iter()
            .any(|d| d.message.contains("E024")));
    }

    #[test]
    fn bind_params_any_type_accepts_anything() {
        let registry = MacroRegistry::new();
        let mut expander = MacroExpander::new(&registry, 10);

        let params = vec![MacroParam {
            name: make_ident("val"),
            type_constraint: Some(TypeExpr::Any(dummy_span())),
            default: None,
            span: dummy_span(),
        }];
        let args = vec![MacroCallArg::Positional(Expr::IntLit(42, dummy_span()))];

        let result = expander.bind_params(&params, &args, dummy_span());
        assert!(result.is_ok());
    }

    // ── Block/Table remove & update tests ────────────────────────────────

    fn make_child_block(kind: &str, id: Option<&str>) -> BodyItem {
        BodyItem::Block(Block {
            decorators: vec![],
            partial: false,
            kind: make_ident(kind),
            inline_id: id.map(|v| {
                InlineId::Literal(IdentifierLit {
                    value: v.to_string(),
                    span: dummy_span(),
                })
            }),
            labels: vec![],
            body: vec![],
            text_content: None,
            trivia: Trivia::empty(),
            span: dummy_span(),
        })
    }

    fn make_table(id: Option<&str>, col_names: &[&str], rows: Vec<Vec<Expr>>) -> BodyItem {
        BodyItem::Table(Table {
            decorators: vec![],
            partial: false,
            inline_id: id.map(|v| {
                InlineId::Literal(IdentifierLit {
                    value: v.to_string(),
                    span: dummy_span(),
                })
            }),
            schema_ref: None,
            columns: col_names
                .iter()
                .map(|n| ColumnDecl {
                    decorators: vec![],
                    name: make_ident(n),
                    type_expr: TypeExpr::String(dummy_span()),
                    trivia: Trivia::empty(),
                    span: dummy_span(),
                })
                .collect(),
            rows: rows
                .into_iter()
                .map(|cells| TableRow {
                    cells,
                    span: dummy_span(),
                })
                .collect(),
            import_expr: None,
            trivia: Trivia::empty(),
            span: dummy_span(),
        })
    }

    fn make_str_expr(s: &str) -> Expr {
        Expr::StringLit(StringLit {
            parts: vec![StringPart::Literal(s.to_string())],
            span: dummy_span(),
        })
    }

    fn make_block_with_children(children: Vec<BodyItem>) -> Block {
        Block {
            decorators: vec![],
            partial: false,
            kind: make_ident("service"),
            inline_id: Some(InlineId::Literal(IdentifierLit {
                value: "main".to_string(),
                span: dummy_span(),
            })),
            labels: vec![],
            body: children,
            text_content: None,
            trivia: Trivia::empty(),
            span: dummy_span(),
        }
    }

    #[test]
    fn remove_block_by_kind_and_id() {
        let registry = MacroRegistry::new();
        let mut expander = MacroExpander::new(&registry, 10);
        let param_bindings = HashMap::new();

        let mut block = make_block_with_children(vec![
            make_child_block("endpoint", Some("health")),
            make_child_block("endpoint", Some("debug")),
        ]);

        let directive = TransformDirective::Remove(RemoveBlock {
            targets: vec![RemoveTarget::Block(
                make_ident("endpoint"),
                IdentifierLit {
                    value: "debug".to_string(),
                    span: dummy_span(),
                },
            )],
            span: dummy_span(),
        });

        expander.apply_directive(&mut block, &directive, &param_bindings);
        assert_eq!(block.body.len(), 1);
        if let BodyItem::Block(b) = &block.body[0] {
            assert!(MacroExpander::matches_inline_id(&b.inline_id, "health"));
        } else {
            panic!("expected Block");
        }
    }

    #[test]
    fn remove_block_all_of_kind() {
        let registry = MacroRegistry::new();
        let mut expander = MacroExpander::new(&registry, 10);
        let param_bindings = HashMap::new();

        let mut block = make_block_with_children(vec![
            make_child_block("endpoint", Some("health")),
            make_child_block("endpoint", Some("debug")),
            BodyItem::Attribute(Attribute {
                decorators: vec![],
                name: make_ident("port"),
                value: Expr::IntLit(8080, dummy_span()),
                trivia: Trivia::empty(),
                span: dummy_span(),
            }),
        ]);

        let directive = TransformDirective::Remove(RemoveBlock {
            targets: vec![RemoveTarget::BlockAll(make_ident("endpoint"))],
            span: dummy_span(),
        });

        expander.apply_directive(&mut block, &directive, &param_bindings);
        assert_eq!(block.body.len(), 1);
        assert!(matches!(&block.body[0], BodyItem::Attribute(a) if a.name.name == "port"));
    }

    #[test]
    fn remove_table_by_id() {
        let registry = MacroRegistry::new();
        let mut expander = MacroExpander::new(&registry, 10);
        let param_bindings = HashMap::new();

        let mut block = make_block_with_children(vec![
            make_table(Some("users"), &["name"], vec![]),
            make_table(Some("metrics"), &["key"], vec![]),
        ]);

        let directive = TransformDirective::Remove(RemoveBlock {
            targets: vec![RemoveTarget::Table(IdentifierLit {
                value: "metrics".to_string(),
                span: dummy_span(),
            })],
            span: dummy_span(),
        });

        expander.apply_directive(&mut block, &directive, &param_bindings);
        assert_eq!(block.body.len(), 1);
    }

    #[test]
    fn remove_all_tables() {
        let registry = MacroRegistry::new();
        let mut expander = MacroExpander::new(&registry, 10);
        let param_bindings = HashMap::new();

        let mut block = make_block_with_children(vec![
            make_table(Some("users"), &["name"], vec![]),
            make_table(Some("metrics"), &["key"], vec![]),
            BodyItem::Attribute(Attribute {
                decorators: vec![],
                name: make_ident("port"),
                value: Expr::IntLit(8080, dummy_span()),
                trivia: Trivia::empty(),
                span: dummy_span(),
            }),
        ]);

        let directive = TransformDirective::Remove(RemoveBlock {
            targets: vec![RemoveTarget::AllTables(dummy_span())],
            span: dummy_span(),
        });

        expander.apply_directive(&mut block, &directive, &param_bindings);
        assert_eq!(block.body.len(), 1);
        assert!(matches!(&block.body[0], BodyItem::Attribute(_)));
    }

    #[test]
    fn remove_mixed_targets() {
        let registry = MacroRegistry::new();
        let mut expander = MacroExpander::new(&registry, 10);
        let param_bindings = HashMap::new();

        let mut block = make_block_with_children(vec![
            BodyItem::Attribute(Attribute {
                decorators: vec![],
                name: make_ident("debug_port"),
                value: Expr::IntLit(9090, dummy_span()),
                trivia: Trivia::empty(),
                span: dummy_span(),
            }),
            make_child_block("endpoint", Some("debug")),
            make_table(Some("metrics"), &["key"], vec![]),
        ]);

        let directive = TransformDirective::Remove(RemoveBlock {
            targets: vec![
                RemoveTarget::Attr(make_ident("debug_port")),
                RemoveTarget::Block(
                    make_ident("endpoint"),
                    IdentifierLit {
                        value: "debug".to_string(),
                        span: dummy_span(),
                    },
                ),
                RemoveTarget::Table(IdentifierLit {
                    value: "metrics".to_string(),
                    span: dummy_span(),
                }),
            ],
            span: dummy_span(),
        });

        expander.apply_directive(&mut block, &directive, &param_bindings);
        assert_eq!(block.body.len(), 0);
    }

    #[test]
    fn remove_block_by_index() {
        let registry = MacroRegistry::new();
        let mut expander = MacroExpander::new(&registry, 10);
        let param_bindings = HashMap::new();

        let mut block = make_block_with_children(vec![
            make_child_block("endpoint", Some("first")),
            make_child_block("endpoint", Some("second")),
            make_child_block("endpoint", Some("third")),
        ]);

        let directive = TransformDirective::Remove(RemoveBlock {
            targets: vec![RemoveTarget::BlockIndex(
                make_ident("endpoint"),
                0,
                dummy_span(),
            )],
            span: dummy_span(),
        });

        expander.apply_directive(&mut block, &directive, &param_bindings);
        assert_eq!(block.body.len(), 2);
        if let BodyItem::Block(b) = &block.body[0] {
            assert!(MacroExpander::matches_inline_id(&b.inline_id, "second"));
        }
    }

    #[test]
    fn remove_table_by_index() {
        let registry = MacroRegistry::new();
        let mut expander = MacroExpander::new(&registry, 10);
        let param_bindings = HashMap::new();

        let mut block = make_block_with_children(vec![
            make_table(Some("first"), &["a"], vec![]),
            make_table(Some("second"), &["b"], vec![]),
        ]);

        let directive = TransformDirective::Remove(RemoveBlock {
            targets: vec![RemoveTarget::TableIndex(1, dummy_span())],
            span: dummy_span(),
        });

        expander.apply_directive(&mut block, &directive, &param_bindings);
        assert_eq!(block.body.len(), 1);
        if let BodyItem::Table(t) = &block.body[0] {
            assert!(MacroExpander::matches_inline_id(&t.inline_id, "first"));
        }
    }

    #[test]
    fn update_block_set_attrs() {
        let registry = MacroRegistry::new();
        let mut expander = MacroExpander::new(&registry, 10);
        let param_bindings = HashMap::new();

        let mut block =
            make_block_with_children(vec![make_child_block("endpoint", Some("health"))]);

        let directive = TransformDirective::Update(UpdateBlock {
            selector: TargetSelector::BlockKindId(
                make_ident("endpoint"),
                IdentifierLit {
                    value: "health".to_string(),
                    span: dummy_span(),
                },
            ),
            block_directives: vec![TransformDirective::Set(SetBlock {
                attrs: vec![Attribute {
                    decorators: vec![],
                    name: make_ident("tls"),
                    value: Expr::BoolLit(true, dummy_span()),
                    trivia: Trivia::empty(),
                    span: dummy_span(),
                }],
                span: dummy_span(),
            })],
            table_directives: vec![],
            span: dummy_span(),
        });

        let changed = expander.apply_directive(&mut block, &directive, &param_bindings);
        assert!(changed);
        if let BodyItem::Block(child) = &block.body[0] {
            assert_eq!(child.body.len(), 1);
            if let BodyItem::Attribute(a) = &child.body[0] {
                assert_eq!(a.name.name, "tls");
            } else {
                panic!("expected attribute");
            }
        }
    }

    #[test]
    fn update_all_blocks_of_kind() {
        let registry = MacroRegistry::new();
        let mut expander = MacroExpander::new(&registry, 10);
        let param_bindings = HashMap::new();

        let mut block = make_block_with_children(vec![
            make_child_block("endpoint", Some("health")),
            make_child_block("endpoint", Some("api")),
        ]);

        let directive = TransformDirective::Update(UpdateBlock {
            selector: TargetSelector::BlockKind(make_ident("endpoint")),
            block_directives: vec![TransformDirective::Inject(InjectBlock {
                body: vec![BodyItem::Attribute(Attribute {
                    decorators: vec![],
                    name: make_ident("auth"),
                    value: Expr::BoolLit(true, dummy_span()),
                    trivia: Trivia::empty(),
                    span: dummy_span(),
                })],
                span: dummy_span(),
            })],
            table_directives: vec![],
            span: dummy_span(),
        });

        expander.apply_directive(&mut block, &directive, &param_bindings);
        // Both endpoints should have auth injected
        for item in &block.body {
            if let BodyItem::Block(child) = item {
                assert_eq!(child.body.len(), 1);
                assert!(matches!(&child.body[0], BodyItem::Attribute(a) if a.name.name == "auth"));
            }
        }
    }

    #[test]
    fn update_block_by_index() {
        let registry = MacroRegistry::new();
        let mut expander = MacroExpander::new(&registry, 10);
        let param_bindings = HashMap::new();

        let mut block = make_block_with_children(vec![
            make_child_block("endpoint", Some("first")),
            make_child_block("endpoint", Some("second")),
        ]);

        let directive = TransformDirective::Update(UpdateBlock {
            selector: TargetSelector::BlockIndex(make_ident("endpoint"), 0, dummy_span()),
            block_directives: vec![TransformDirective::Set(SetBlock {
                attrs: vec![Attribute {
                    decorators: vec![],
                    name: make_ident("primary"),
                    value: Expr::BoolLit(true, dummy_span()),
                    trivia: Trivia::empty(),
                    span: dummy_span(),
                }],
                span: dummy_span(),
            })],
            table_directives: vec![],
            span: dummy_span(),
        });

        expander.apply_directive(&mut block, &directive, &param_bindings);
        if let BodyItem::Block(child) = &block.body[0] {
            assert_eq!(child.body.len(), 1);
            assert!(matches!(&child.body[0], BodyItem::Attribute(a) if a.name.name == "primary"));
        }
        // Second endpoint should be unchanged
        if let BodyItem::Block(child) = &block.body[1] {
            assert!(child.body.is_empty());
        }
    }

    #[test]
    fn update_nested_in_when() {
        let mut registry = MacroRegistry::new();
        let macro_def = MacroDef {
            decorators: vec![],
            kind: MacroKind::Attribute,
            name: make_ident("secure"),
            params: vec![],
            body: MacroBody::Attribute(vec![TransformDirective::When(WhenBlock {
                condition: Expr::BoolLit(true, dummy_span()),
                directives: vec![TransformDirective::Update(UpdateBlock {
                    selector: TargetSelector::BlockKindId(
                        make_ident("endpoint"),
                        IdentifierLit {
                            value: "health".to_string(),
                            span: dummy_span(),
                        },
                    ),
                    block_directives: vec![TransformDirective::Set(SetBlock {
                        attrs: vec![Attribute {
                            decorators: vec![],
                            name: make_ident("tls"),
                            value: Expr::BoolLit(true, dummy_span()),
                            trivia: Trivia::empty(),
                            span: dummy_span(),
                        }],
                        span: dummy_span(),
                    })],
                    table_directives: vec![],
                    span: dummy_span(),
                })],
                span: dummy_span(),
            })]),
            trivia: Trivia::empty(),
            span: dummy_span(),
        };
        registry
            .attribute_macros
            .insert("secure".to_string(), macro_def);

        let mut block =
            make_block_with_children(vec![make_child_block("endpoint", Some("health"))]);
        block.decorators.push(Decorator {
            name: make_ident("secure"),
            args: vec![],
            span: dummy_span(),
        });

        let mut expander = MacroExpander::new(&registry, 10);
        expander.apply_attribute_macros(&mut block);

        if let BodyItem::Block(child) = &block.body[0] {
            assert_eq!(child.body.len(), 1);
            assert!(matches!(&child.body[0], BodyItem::Attribute(a) if a.name.name == "tls"));
        }
    }

    #[test]
    fn update_table_inject_rows() {
        let registry = MacroRegistry::new();
        let mut expander = MacroExpander::new(&registry, 10);
        let param_bindings = HashMap::new();

        let mut block = make_block_with_children(vec![make_table(
            Some("users"),
            &["name", "age"],
            vec![vec![make_str_expr("alice"), Expr::IntLit(25, dummy_span())]],
        )]);

        let directive = TransformDirective::Update(UpdateBlock {
            selector: TargetSelector::TableId(IdentifierLit {
                value: "users".to_string(),
                span: dummy_span(),
            }),
            block_directives: vec![],
            table_directives: vec![TableDirective::InjectRows(
                vec![TableRow {
                    cells: vec![make_str_expr("bob"), Expr::IntLit(30, dummy_span())],
                    span: dummy_span(),
                }],
                dummy_span(),
            )],
            span: dummy_span(),
        });

        expander.apply_directive(&mut block, &directive, &param_bindings);
        if let BodyItem::Table(t) = &block.body[0] {
            assert_eq!(t.rows.len(), 2);
        }
    }

    #[test]
    fn update_table_remove_rows() {
        let registry = MacroRegistry::new();
        let mut expander = MacroExpander::new(&registry, 10);
        let param_bindings = HashMap::new();

        let mut block = make_block_with_children(vec![make_table(
            Some("users"),
            &["name", "role"],
            vec![
                vec![make_str_expr("alice"), make_str_expr("admin")],
                vec![make_str_expr("bob"), make_str_expr("guest")],
            ],
        )]);

        // remove_rows where role == "guest"
        let condition = Expr::BinaryOp(
            Box::new(Expr::Ident(make_ident("role"))),
            BinOp::Eq,
            Box::new(make_str_expr("guest")),
            dummy_span(),
        );

        let directive = TransformDirective::Update(UpdateBlock {
            selector: TargetSelector::TableId(IdentifierLit {
                value: "users".to_string(),
                span: dummy_span(),
            }),
            block_directives: vec![],
            table_directives: vec![TableDirective::RemoveRows {
                condition,
                span: dummy_span(),
            }],
            span: dummy_span(),
        });

        expander.apply_directive(&mut block, &directive, &param_bindings);
        if let BodyItem::Table(t) = &block.body[0] {
            assert_eq!(t.rows.len(), 1);
            // Remaining row should be alice/admin
            if let Expr::StringLit(s) = &t.rows[0].cells[0] {
                if let StringPart::Literal(val) = &s.parts[0] {
                    assert_eq!(val, "alice");
                }
            }
        }
    }

    #[test]
    fn update_table_update_rows() {
        let registry = MacroRegistry::new();
        let mut expander = MacroExpander::new(&registry, 10);
        let param_bindings = HashMap::new();

        let mut block = make_block_with_children(vec![make_table(
            Some("users"),
            &["name", "status"],
            vec![
                vec![make_str_expr("alice"), make_str_expr("active")],
                vec![make_str_expr("bob"), make_str_expr("active")],
            ],
        )]);

        // update_rows where name == "bob" { set { status = "inactive" } }
        let condition = Expr::BinaryOp(
            Box::new(Expr::Ident(make_ident("name"))),
            BinOp::Eq,
            Box::new(make_str_expr("bob")),
            dummy_span(),
        );

        let directive = TransformDirective::Update(UpdateBlock {
            selector: TargetSelector::TableId(IdentifierLit {
                value: "users".to_string(),
                span: dummy_span(),
            }),
            block_directives: vec![],
            table_directives: vec![TableDirective::UpdateRows {
                condition,
                attrs: vec![(make_ident("status"), make_str_expr("inactive"))],
                span: dummy_span(),
            }],
            span: dummy_span(),
        });

        expander.apply_directive(&mut block, &directive, &param_bindings);
        if let BodyItem::Table(t) = &block.body[0] {
            // bob's status should be "inactive"
            if let Expr::StringLit(s) = &t.rows[1].cells[1] {
                if let StringPart::Literal(val) = &s.parts[0] {
                    assert_eq!(val, "inactive");
                }
            }
        }
    }

    #[test]
    fn update_table_clear_rows() {
        let registry = MacroRegistry::new();
        let mut expander = MacroExpander::new(&registry, 10);
        let param_bindings = HashMap::new();

        let mut block = make_block_with_children(vec![make_table(
            Some("users"),
            &["name"],
            vec![vec![make_str_expr("alice")], vec![make_str_expr("bob")]],
        )]);

        let directive = TransformDirective::Update(UpdateBlock {
            selector: TargetSelector::TableId(IdentifierLit {
                value: "users".to_string(),
                span: dummy_span(),
            }),
            block_directives: vec![],
            table_directives: vec![TableDirective::ClearRows(dummy_span())],
            span: dummy_span(),
        });

        expander.apply_directive(&mut block, &directive, &param_bindings);
        if let BodyItem::Table(t) = &block.body[0] {
            assert_eq!(t.rows.len(), 0);
            assert_eq!(t.columns.len(), 1); // columns preserved
        }
    }

    #[test]
    fn update_table_by_index() {
        let registry = MacroRegistry::new();
        let mut expander = MacroExpander::new(&registry, 10);
        let param_bindings = HashMap::new();

        let mut block = make_block_with_children(vec![
            make_table(Some("first"), &["a"], vec![vec![make_str_expr("x")]]),
            make_table(Some("second"), &["b"], vec![vec![make_str_expr("y")]]),
        ]);

        let directive = TransformDirective::Update(UpdateBlock {
            selector: TargetSelector::TableIndex(0, dummy_span()),
            block_directives: vec![],
            table_directives: vec![TableDirective::ClearRows(dummy_span())],
            span: dummy_span(),
        });

        expander.apply_directive(&mut block, &directive, &param_bindings);
        if let BodyItem::Table(t) = &block.body[0] {
            assert_eq!(t.rows.len(), 0); // first table cleared
        }
        if let BodyItem::Table(t) = &block.body[1] {
            assert_eq!(t.rows.len(), 1); // second table untouched
        }
    }
}
