mod compiler;
mod config;
mod directory;
mod executable;
mod node;

use std::cell::RefCell;
use std::env::args;
use std::env::current_dir;
use std::error::Error;
use std::fs::File;
use std::io;
use std::io::BufReader;
use std::path::PathBuf;
use std::rc::Rc;

use color_eyre::Report;
use compiler::CSourceToObject;
use compiler::LinkObjectsToBinary;
use directory::CreateDirectory;
use itertools::Itertools;
use node::Node;
use walkdir::WalkDir;

use crate::config::Project;

fn main() -> Result<(), Report> {
    color_eyre::install()?;

    let args = args().collect::<Vec<_>>();
    match args.get(1).map(|f| f.as_str()) {
        Some("build") => {
            build_project()?;
        },

        Some("-v" | "--version") => {
            #[rustfmt::skip]
            println!(
                "The Loki Build System, version 0.1.0\n\
                \n\
                Copyright (c) 2023 Reperak\n\
                \n\
                Loki is free software licensed under the GNU GPL version 3 or later.\n\
                \n\
                If you did not receive a copy of the license with this program, you may obtain\n\
                one at <http://gnu.org/licenses/gpl.html>."
            );
        },

        Some("-h" | "--help") | None => {
            #[rustfmt::skip]
            println!(
                "The Loki Build System\n\
                \n\
                Copyright (c) 2023 Reperak\n\
                \n\
                Subcommands:\n    \
                    build           Build a Loki project\n\
                \n\
                Usage:\n    \
                    --help          Show this text and exit\n    \
                    --version       Show version information"
            );
        },

        _ => {
            println!("Unknown command/flag '{}'. See '--help' for usage.", args[1]);
        },
    }

    Ok(())
}

fn build_project() -> Result<(), Report> {
    let (loki_toml, source_directory, target_directory, object_directory) = current_dir()?
        .ancestors()
        .map(PathBuf::from)
        .map(|project_directory| {
            (
                project_directory.join("loki.toml"),
                project_directory.join("src"),
                project_directory.join("target"),
                project_directory.join("target/obj"),
            )
        })
        .filter(|(loki_toml, ..)| loki_toml.exists())
        .last()
        .ok_or(io::Error::new(
            io::ErrorKind::NotFound,
            "loki project directory not found",
        ))?;

    let project: Project = toml::from_str(&io::read_to_string(BufReader::new(File::open(loki_toml)?))?)?;

    let source_files = WalkDir::new(source_directory)
        .into_iter()
        .map(|dir| dir.unwrap().into_path())
        .filter(|path| path.extension().is_some_and(|d| d == "c"))
        .collect_vec();

    let create_target_directory_node = Rc::new(RefCell::new(Node {
        executable: Box::new(CreateDirectory {
            directory: target_directory.clone(),
        }),
        children:   Vec::new(),
    }));

    let create_object_directory_node = Rc::new(RefCell::new(Node {
        executable: Box::new(CreateDirectory {
            directory: object_directory.clone(),
        }),
        children:   Vec::new(),
    }));

    let c2so_nodes = source_files
        .clone()
        .into_iter()
        .map(|source| {
            let cs2o = CSourceToObject {
                configuration:    project.configuration,
                input:            source,
                object_directory: object_directory.clone(),
            };

            let node = Node {
                executable: Box::new(cs2o),
                children:   vec![
                    Rc::clone(&create_target_directory_node),
                    Rc::clone(&create_object_directory_node),
                ],
            };

            Rc::new(RefCell::new(node))
        })
        .collect_vec();

    let lo2b_node = Rc::new(RefCell::new(Node {
        executable: Box::new(LinkObjectsToBinary {
            optimization: project.configuration.optimization,
            inputs:       source_files,
            output:       target_directory.join(project.package.name),
        }),
        children:   [&c2so_nodes[..], &[
            Rc::clone(&create_target_directory_node),
            Rc::clone(&create_target_directory_node),
        ]]
        .concat(),
    }));

    execute_node(lo2b_node).unwrap();

    Ok(())
}

fn execute_node(node: Rc<RefCell<Node>>) -> Result<i32, Box<dyn Error + Send + Sync>> {
    for child in &node.borrow().children {
        execute_node(Rc::clone(child))?;
    }

    node.borrow_mut().executable.execute()
}
