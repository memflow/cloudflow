use anyhow::Result;
use memflow_framework::*;
use ptree::{print_tree, Style, TreeItem};
use std::borrow::Cow;

#[derive(Clone)]
enum TreeNode {
    Leaf(String),
    Branch(String, Vec<TreeNode>),
}

impl TreeItem for TreeNode {
    type Child = Self;

    fn write_self<W: std::io::Write>(&self, f: &mut W, style: &Style) -> std::io::Result<()> {
        write!(
            f,
            "{}",
            style.paint(match self {
                TreeNode::Leaf(l) | TreeNode::Branch(l, _) => l,
            })
        )
    }

    fn children(&self) -> Cow<[Self::Child]> {
        Cow::from(match self {
            TreeNode::Leaf(_) => vec![],
            TreeNode::Branch(_, children) => children.clone(),
        })
    }
}

fn build_tree(node: &Node, path: &mut Vec<String>, tree: &mut TreeNode) -> Result<()> {
    if let TreeNode::Branch(_, children) = tree {
        for (p, is_branch) in node.list(&path.join("/"))? {
            let n = if is_branch {
                let mut n = TreeNode::Branch(p.clone(), vec![]);
                path.push(p);
                build_tree(node, path, &mut n)?;
                path.pop();
                n
            } else {
                TreeNode::Leaf(p)
            };

            children.push(n);
        }
    }

    Ok(())
}

fn main() -> Result<()> {
    let node = Node::default();

    let mut root = TreeNode::Branch("/".into(), vec![]);

    build_tree(&node, &mut vec![], &mut root)?;

    print_tree(&root)?;

    let handle = node.open("os/native/rpc")?;

    Ok(())
}
