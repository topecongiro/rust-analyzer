//! FIXME: write short doc here

use std::{
    convert::From,
    fmt::{self, Display},
};

use hir::{Docs, Documentation, HasSource, HirDisplay};
use ra_ide_db::RootDatabase;
use ra_syntax::ast::{self, AstNode, NameOwner, VisibilityOwner};
use stdx::SepBy;

use crate::display::{generic_parameters, where_predicates};

#[derive(Debug)]
pub enum CallableKind {
    Function,
    StructConstructor,
    VariantConstructor,
    Macro,
}

/// Contains information about a function signature
#[derive(Debug)]
pub struct FunctionSignature {
    pub kind: CallableKind,
    /// Optional visibility
    pub visibility: Option<String>,
    /// Name of the function
    pub name: Option<String>,
    /// Documentation for the function
    pub doc: Option<Documentation>,
    /// Generic parameters
    pub generic_parameters: Vec<String>,
    /// Parameters of the function
    pub parameters: Vec<String>,
    /// Parameter names of the function
    pub parameter_names: Vec<String>,
    /// Parameter types of the function
    pub parameter_types: Vec<String>,
    /// Optional return type
    pub ret_type: Option<String>,
    /// Where predicates
    pub where_predicates: Vec<String>,
    /// Self param presence
    pub has_self_param: bool,
}

impl FunctionSignature {
    pub(crate) fn with_doc_opt(mut self, doc: Option<Documentation>) -> Self {
        self.doc = doc;
        self
    }

    pub(crate) fn from_hir(db: &RootDatabase, function: hir::Function) -> Self {
        let doc = function.docs(db);
        let ast_node = function.source(db).value;
        FunctionSignature::from(&ast_node).with_doc_opt(doc)
    }

    pub(crate) fn from_struct(db: &RootDatabase, st: hir::Struct) -> Option<Self> {
        let node: ast::StructDef = st.source(db).value;
        if let ast::StructKind::Record(_) = node.kind() {
            return None;
        };

        let mut params = vec![];
        let mut parameter_types = vec![];
        for field in st.fields(db).into_iter() {
            let ty = field.signature_ty(db);
            let raw_param = format!("{}", ty.display(db));

            if let Some(param_type) = raw_param.split(':').nth(1) {
                parameter_types.push(param_type[1..].to_string());
            } else {
                // useful when you have tuple struct
                parameter_types.push(raw_param.clone());
            }
            params.push(raw_param);
        }

        Some(
            FunctionSignature {
                kind: CallableKind::StructConstructor,
                visibility: node.visibility().map(|n| n.syntax().text().to_string()),
                name: node.name().map(|n| n.text().to_string()),
                ret_type: node.name().map(|n| n.text().to_string()),
                parameters: params,
                parameter_names: vec![],
                parameter_types,
                generic_parameters: generic_parameters(&node),
                where_predicates: where_predicates(&node),
                doc: None,
                has_self_param: false,
            }
            .with_doc_opt(st.docs(db)),
        )
    }

    pub(crate) fn from_enum_variant(db: &RootDatabase, variant: hir::EnumVariant) -> Option<Self> {
        let node: ast::EnumVariant = variant.source(db).value;
        match node.kind() {
            ast::StructKind::Record(_) | ast::StructKind::Unit => return None,
            _ => (),
        };

        let parent_name = variant.parent_enum(db).name(db).to_string();

        let name = format!("{}::{}", parent_name, variant.name(db));

        let mut params = vec![];
        let mut parameter_types = vec![];
        for field in variant.fields(db).into_iter() {
            let ty = field.signature_ty(db);
            let raw_param = format!("{}", ty.display(db));
            if let Some(param_type) = raw_param.split(':').nth(1) {
                parameter_types.push(param_type[1..].to_string());
            } else {
                // The unwrap_or_else is useful when you have tuple
                parameter_types.push(raw_param);
            }
            let name = field.name(db);

            params.push(format!("{}: {}", name, ty.display(db)));
        }

        Some(
            FunctionSignature {
                kind: CallableKind::VariantConstructor,
                visibility: None,
                name: Some(name),
                ret_type: None,
                parameters: params,
                parameter_names: vec![],
                parameter_types,
                generic_parameters: vec![],
                where_predicates: vec![],
                doc: None,
                has_self_param: false,
            }
            .with_doc_opt(variant.docs(db)),
        )
    }

    pub(crate) fn from_macro(db: &RootDatabase, macro_def: hir::MacroDef) -> Option<Self> {
        let node: ast::MacroCall = macro_def.source(db).value;

        let params = vec![];

        Some(
            FunctionSignature {
                kind: CallableKind::Macro,
                visibility: None,
                name: node.name().map(|n| n.text().to_string()),
                ret_type: None,
                parameters: params,
                parameter_names: vec![],
                parameter_types: vec![],
                generic_parameters: vec![],
                where_predicates: vec![],
                doc: None,
                has_self_param: false,
            }
            .with_doc_opt(macro_def.docs(db)),
        )
    }
}

impl From<&'_ ast::FnDef> for FunctionSignature {
    fn from(node: &ast::FnDef) -> FunctionSignature {
        fn param_list(node: &ast::FnDef) -> (bool, Vec<String>, Vec<String>) {
            let mut res = vec![];
            let mut res_types = vec![];
            let mut has_self_param = false;
            if let Some(param_list) = node.param_list() {
                if let Some(self_param) = param_list.self_param() {
                    has_self_param = true;
                    let raw_param = self_param.syntax().text().to_string();

                    res_types.push(
                        raw_param.split(':').nth(1).unwrap_or_else(|| " Self")[1..].to_string(),
                    );
                    res.push(raw_param);
                }

                res.extend(param_list.params().map(|param| param.syntax().text().to_string()));
                res_types.extend(param_list.params().map(|param| {
                    param.syntax().text().to_string().split(':').nth(1).unwrap()[1..].to_string()
                }));
            }
            (has_self_param, res, res_types)
        }

        fn param_name_list(node: &ast::FnDef) -> Vec<String> {
            let mut res = vec![];
            if let Some(param_list) = node.param_list() {
                if let Some(self_param) = param_list.self_param() {
                    res.push(self_param.syntax().text().to_string())
                }

                res.extend(
                    param_list
                        .params()
                        .map(|param| {
                            Some(
                                param
                                    .pat()?
                                    .syntax()
                                    .descendants()
                                    .find_map(ast::Name::cast)?
                                    .text()
                                    .to_string(),
                            )
                        })
                        .map(|param| param.unwrap_or_default()),
                );
            }
            res
        }

        let (has_self_param, parameters, parameter_types) = param_list(node);

        FunctionSignature {
            kind: CallableKind::Function,
            visibility: node.visibility().map(|n| n.syntax().text().to_string()),
            name: node.name().map(|n| n.text().to_string()),
            ret_type: node
                .ret_type()
                .and_then(|r| r.type_ref())
                .map(|n| n.syntax().text().to_string()),
            parameters,
            parameter_names: param_name_list(node),
            parameter_types,
            generic_parameters: generic_parameters(node),
            where_predicates: where_predicates(node),
            // docs are processed separately
            doc: None,
            has_self_param,
        }
    }
}

impl Display for FunctionSignature {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if let Some(t) = &self.visibility {
            write!(f, "{} ", t)?;
        }

        if let Some(name) = &self.name {
            match self.kind {
                CallableKind::Function => write!(f, "fn {}", name)?,
                CallableKind::StructConstructor => write!(f, "struct {}", name)?,
                CallableKind::VariantConstructor => write!(f, "{}", name)?,
                CallableKind::Macro => write!(f, "{}!", name)?,
            }
        }

        if !self.generic_parameters.is_empty() {
            write!(f, "{}", self.generic_parameters.iter().sep_by(", ").surround_with("<", ">"))?;
        }

        write!(f, "{}", self.parameters.iter().sep_by(", ").surround_with("(", ")"))?;

        if let Some(t) = &self.ret_type {
            write!(f, " -> {}", t)?;
        }

        if !self.where_predicates.is_empty() {
            write!(f, "\nwhere {}", self.where_predicates.iter().sep_by(",\n      "))?;
        }

        Ok(())
    }
}
