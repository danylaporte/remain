use quote::quote;
use std::cmp::Ordering;
use syn::{Arm, Attribute, Ident, ImplItem, Result, Variant};
use syn::{Error, Field, Pat, PatIdent};

use crate::compare::{cmp, Path, UnderscoreOrder};
use crate::format;
use crate::parse::Input::{self, *};

pub fn sorted(input: &mut Input) -> Result<()> {
    let paths = match input {
        Enum(item) => collect_paths(&mut item.variants)?,
        Impl(item) => collect_paths(&mut item.items)?,
        Struct(item) => collect_paths(&mut item.fields)?,
        Match(expr) | Let(expr) => collect_paths(&mut expr.arms)?,
    };

    let mode = UnderscoreOrder::First;
    if find_misordered(&paths, mode).is_none() {
        return Ok(());
    }

    let mode = UnderscoreOrder::Last;
    let wrong = match find_misordered(&paths, mode) {
        Some(wrong) => wrong,
        None => return Ok(()),
    };

    let lesser = &paths[wrong];
    let correct_pos = match paths[..wrong - 1].binary_search_by(|probe| cmp(probe, lesser, mode)) {
        Err(correct_pos) => correct_pos,
        Ok(equal_to) => equal_to + 1,
    };
    let greater = &paths[correct_pos];
    Err(format::error(&lesser.1, &greater.1))
}

fn find_misordered(paths: &[(Category, Path)], mode: UnderscoreOrder) -> Option<usize> {
    for i in 1..paths.len() {
        if cmp(&paths[i], &paths[i - 1], mode) == Ordering::Less {
            return Some(i);
        }
    }

    None
}

fn collect_paths<'a, I, P>(iter: I) -> Result<Vec<(Category, Path)>>
where
    I: IntoIterator<Item = &'a mut P>,
    P: Sortable + 'a,
{
    iter.into_iter()
        .filter_map(|item| {
            if remove_unsorted_attr(item.attrs()) {
                None
            } else {
                Some(item.to_path().map(|path| (item.category(), path)))
            }
        })
        .collect()
}

fn remove_unsorted_attr(attrs: &mut Vec<Attribute>) -> bool {
    for i in 0..attrs.len() {
        let path = &attrs[i].path;
        let path = quote!(#path).to_string();
        if path == "unsorted" || path == "remain :: unsorted" {
            attrs.remove(i);
            return true;
        }
    }

    false
}

trait Sortable {
    fn to_path(&self) -> Result<Path>;
    fn attrs(&mut self) -> &mut Vec<Attribute>;

    fn category(&self) -> u8 {
        0
    }
}

impl Sortable for Variant {
    fn to_path(&self) -> Result<Path> {
        Ok(Path {
            segments: vec![self.ident.clone()],
        })
    }
    fn attrs(&mut self) -> &mut Vec<Attribute> {
        &mut self.attrs
    }
}

impl Sortable for Field {
    fn to_path(&self) -> Result<Path> {
        Ok(Path {
            segments: vec![self.ident.clone().expect("must be named field")],
        })
    }
    fn attrs(&mut self) -> &mut Vec<Attribute> {
        &mut self.attrs
    }
}

impl Sortable for ImplItem {
    fn to_path(&self) -> Result<Path> {
        let segments = match self {
            Self::Const(c) => vec![c.ident.clone()],
            Self::Type(t) => vec![t.ident.clone()],
            Self::Macro(m) => idents_of_path(&m.mac.path),
            Self::Method(m) => vec![m.sig.ident.clone()],
            other => {
                let msg = "unsupported by #[remain::sorted]";
                return Err(Error::new_spanned(other, msg));
            }
        };

        Ok(Path { segments })
    }

    fn attrs(&mut self) -> &mut Vec<Attribute> {
        match self {
            Self::Const(c) => &mut c.attrs,
            Self::Macro(m) => &mut m.attrs,
            Self::Method(m) => &mut m.attrs,
            Self::Type(t) => &mut t.attrs,
            _ => panic!("unsupported attributes"),
        }
    }

    fn category(&self) -> Category {
        match self {
            Self::Const(_) => 0,
            Self::Type(_) => 1,
            Self::Method(_) => 2,
            Self::Macro(_) => 3,
            _ => 4,
        }
    }
}

impl Sortable for Arm {
    fn to_path(&self) -> Result<Path> {
        // Sort by just the first pat.
        let pat = match &self.pat {
            Pat::Or(pat) => pat.cases.iter().next().expect("at least one pat"),
            _ => &self.pat,
        };

        let segments = match pat {
            Pat::Ident(pat) if is_just_ident(pat) => vec![pat.ident.clone()],
            Pat::Path(pat) => idents_of_path(&pat.path),
            Pat::Struct(pat) => idents_of_path(&pat.path),
            Pat::TupleStruct(pat) => idents_of_path(&pat.path),
            Pat::Wild(pat) => vec![Ident::from(pat.underscore_token)],
            other => {
                let msg = "unsupported by #[remain::sorted]";
                return Err(Error::new_spanned(other, msg));
            }
        };

        Ok(Path { segments })
    }
    fn attrs(&mut self) -> &mut Vec<Attribute> {
        &mut self.attrs
    }
}

fn idents_of_path(path: &syn::Path) -> Vec<Ident> {
    path.segments.iter().map(|seg| seg.ident.clone()).collect()
}

fn is_just_ident(pat: &PatIdent) -> bool {
    pat.by_ref.is_none() && pat.mutability.is_none() && pat.subpat.is_none()
}

pub(crate) type Category = u8;
