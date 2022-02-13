use anyhow::Result;
use filer::prelude::v1::*;
use memflow::prelude::v1::{Address, *};
use memflow_framework::*;
use ptree::{print_tree, Style, TreeItem};
use rand::{Rng, SeedableRng};
use rand_xorshift::XorShiftRng as CurRng;
use std::borrow::Cow;
use std::io::Write;
use std::time::{Duration, Instant};

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
        node.list(
            &path.join("/"),
            &mut (&mut |data: ListEntry| {
                let p = data.name.to_string();
                let is_branch = data.is_branch;

                let n = if is_branch {
                    let mut n = TreeNode::Branch(p.clone(), vec![]);
                    path.push(p);
                    let _ = build_tree(node, path, &mut n).unwrap();
                    path.pop();
                    n
                } else {
                    TreeNode::Leaf(p)
                };

                children.push(n);

                true
            })
                .into(),
        )?;
    }

    Ok(())
}

fn rwtest2(
    mem: &mut impl MemoryView,
    addr: Address,
    chunk_sizes: &[usize],
    chunk_counts: &[usize],
    read_size: usize,
) {
    let mut rng = CurRng::seed_from_u64(0);

    println!("Performance bench:");
    print!("{:#7}", "SIZE");

    for i in chunk_counts {
        print!(", x{:02x} mb/s, x{:02x} calls/s", *i, *i);
    }

    println!();

    let start = Instant::now();
    let mut ttdur = Duration::new(0, 0);

    for i in chunk_sizes {
        print!("0x{:05x}", *i);
        for o in chunk_counts {
            let mut done_size = 0_usize;
            let mut total_dur = Duration::new(0, 0);
            let mut calls = 0;
            let mut bufs = vec![(vec![0_u8; *i], 0); *o];

            let base_addr = addr.to_umem();

            // This code will increase the read size for higher number of chunks
            // Since optimized vtop should scale very well with chunk sizes.
            assert!((i.trailing_zeros() as usize) < usize::MAX);
            let chunk_multiplier = *o * (i.trailing_zeros() as usize + 1);

            while done_size < read_size * chunk_multiplier {
                let mut read_data = vec![];

                for (buf, addr) in bufs.iter_mut() {
                    *addr = base_addr + rng.gen_range(0..0x1000);
                    read_data.push(MemData(Address::from(*addr), buf.as_mut_slice().into()));
                }

                let mut iter = read_data.into_iter();

                let now = Instant::now();
                mem.read_raw_iter((&mut iter).into(), &mut (&mut |_| true).into())
                    .expect("Failure");
                total_dur += now.elapsed();
                done_size += *i * *o;
                calls += 1;
            }

            ttdur += total_dur;
            let total_time = total_dur.as_secs_f64();

            print!(
                ", {:8.2}, {:11.2}",
                (done_size / 0x0010_0000) as f64 / total_time,
                calls as f64 / total_time
            );
            std::io::stdout().flush().expect("");
        }
        println!();
    }

    let total_dur = start.elapsed();
    println!(
        "Total bench time: {:.2} {:.2}",
        total_dur.as_secs_f64(),
        ttdur.as_secs_f64()
    );
}

fn rwtest(
    frontend: &impl Frontend,
    handle: usize,
    addr: Size,
    chunk_sizes: &[usize],
    chunk_counts: &[usize],
    read_size: usize,
) {
    let mut rng = CurRng::seed_from_u64(0);

    println!("Performance bench:");
    print!("{:#7}", "SIZE");

    for i in chunk_counts {
        print!(", x{:02x} mb/s, x{:02x} calls/s", *i, *i);
    }

    println!();

    let start = Instant::now();
    let mut ttdur = Duration::new(0, 0);

    for i in chunk_sizes {
        print!("0x{:05x}", *i);
        for o in chunk_counts {
            let mut done_size = 0_usize;
            let mut total_dur = Duration::new(0, 0);
            let mut calls = 0;
            let mut bufs = vec![(vec![0_u8; *i], 0); *o];

            let base_addr = addr.to_umem();

            // This code will increase the read size for higher number of chunks
            // Since optimized vtop should scale very well with chunk sizes.
            assert!((i.trailing_zeros() as usize) < usize::MAX);
            let chunk_multiplier = *o * (i.trailing_zeros() as usize + 1);

            while done_size < read_size * chunk_multiplier {
                let mut read_data = vec![];

                for (buf, addr) in bufs.iter_mut() {
                    *addr = base_addr + rng.gen_range(0..0x1000);
                    read_data.push(CTup2(*addr as _, buf.as_mut_slice().into()));
                }

                let mut iter = read_data.into_iter();

                let now = Instant::now();
                frontend.read(handle, (&mut iter).into()).expect("Failure");
                total_dur += now.elapsed();
                done_size += *i * *o;
                calls += 1;
            }

            ttdur += total_dur;
            let total_time = total_dur.as_secs_f64();

            print!(
                ", {:8.2}, {:11.2}",
                (done_size / 0x0010_0000) as f64 / total_time,
                calls as f64 / total_time
            );
            std::io::stdout().flush().expect("");
        }
        println!();
    }

    let total_dur = start.elapsed();
    println!(
        "Total bench time: {:.2} {:.2}",
        total_dur.as_secs_f64(),
        ttdur.as_secs_f64()
    );
}

fn main() -> Result<()> {
    println!("Create node");

    let node = create_node();

    println!("Create connectors");

    let mut conn_new = node.open_cursor("connector/new")?;
    write!(conn_new, "kcore kcore")?;
    write!(conn_new, "qemu_win10 qemu:win10-hw")?;

    let mut os_new = node.open_cursor("os/new")?;
    write!(os_new, "native native")?;

    println!("List tree");

    let mut root = TreeNode::Branch("/".into(), vec![]);

    build_tree(&node, &mut vec![], &mut root)?;

    print_tree(&root)?;

    let handle = node.open("connector/kcore/mem")?;

    println!("Handle: {:x}", handle);

    let mut buf = vec![0; 4096];

    let cr3 = 0x3dce10000u64;

    let mut iter = std::iter::once(CTup2(cr3, CSliceMut::from(buf.as_mut_slice())));

    let iter = (&mut iter).into();

    node.read(handle, iter)?;

    for chunk in buf.chunks(8) {
        let v = u64::from_ne_bytes(<[u8; 8]>::try_from(chunk).unwrap());
        print!("{:x}, ", v);
    }

    println!();

    rwtest(
        &node,
        handle,
        cr3,
        &[0x10000, 0x1000, 0x100, 0x10, 0x8],
        &[32, 8, 1],
        0x0010_0000,
    );

    rwtest2(
        &mut Inventory::scan()
            .create_connector("kcore", None, None)?
            .into_phys_view(),
        cr3.into(),
        &[0x10000, 0x1000, 0x100, 0x10, 0x8],
        &[32, 8, 1],
        0x0010_0000,
    );

    Ok(())
}
