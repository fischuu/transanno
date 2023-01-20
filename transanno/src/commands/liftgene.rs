use anyhow::Context;
use autocompress::{create, open, CompressionLevel};
use clap::Args;
use liftover::genelift::GeneLiftOver;
use liftover::geneparse::gff3::{Gff3GroupedReader, Gff3Reader};
use liftover::geneparse::gtf::{GtfGroupedReader, GtfReader};
use liftover::geneparse::{Feature, GroupedReader};
use liftover::poslift::PositionLiftOver;
use liftover::LiftOverError;
use std::fmt::Display;
use std::io;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, clap::ValueEnum)]
enum ArgFormat {
    Auto,
    GFF3,
    GTF,
}

#[derive(Debug, Clone, Args)]
#[command(about = "Lift GENCODE or Ensemble GFF3/GTF file")]
pub struct LiftGene {
    #[arg(long = "chain", short = 'c', help = "chain file")]
    chain: String,
    #[arg(help = "input GFF3/GTF file (GENCODE/Ensemble)")]
    gff: String,
    #[arg(
        long = "format",
        short = 'f',
        default_value = "auto",
        help = "Input file format"
    )]
    format: ArgFormat,
    #[arg(long = "output", short = 'o', help = "GFF3/GTF output path (unsorted)")]
    output: String,
    #[arg(
        long = "failed",
        short = 'f',
        help = "Failed to liftOver GFF3/GTF output path"
    )]
    failed: String,
}

impl LiftGene {
    // fn command_name(&self) -> &'static str {
    //     "liftgene"
    // }
    // fn config_subcommand(&self, app: App<'static, 'static>) -> App<'static, 'static> {
    //     app.about("Lift GENCODE or Ensemble GFF3/GTF file")
    //         .arg(
    //             Arg::with_name("chain")
    //                 .long("chain")
    //                 .short("c")
    //                 .required(true)
    //                 .takes_value(true)
    //                 .help("chain file"),
    //         )
    //         .arg(
    //             Arg::with_name("gff")
    //                 .index(1)
    //                 .required(true)
    //                 .takes_value(true)
    //                 .help("input GFF3/GTF file (GENCODE/Ensemble)"),
    //         )
    //         .arg(
    //             Arg::with_name("format")
    //                 .long("format")
    //                 .possible_values(&["auto", "GFF3", "GTF"])
    //                 .takes_value(true)
    //                 .default_value("auto")
    //                 .help("Input file format"),
    //         )
    //         .arg(
    //             Arg::with_name("output")
    //                 .long("output")
    //                 .short("o")
    //                 .required(true)
    //                 .takes_value(true)
    //                 .help("GFF3/GTF output path (unsorted)"),
    //         )
    //         .arg(
    //             Arg::with_name("failed")
    //                 .long("failed")
    //                 .short("f")
    //                 .required(true)
    //                 .takes_value(true)
    //                 .help("Failed to liftOver GFF3/GTF output path"),
    //         )
    // }
    pub fn run(&self) -> anyhow::Result<()> {
        lift_gene_helper(
            &self.chain,
            &self.gff,
            self.format,
            &self.output,
            &self.failed,
        )?;
        Ok(())
    }
}

// pub fn lift_gene(matches: &ArgMatches) {
//     lift_gene_helper(
//         matches.value_of("chain").unwrap(),
//         matches.value_of("gff").unwrap(),
//         matches.value_of("format").unwrap(),
//         matches.value_of("output").unwrap(),
//         matches.value_of("failed").unwrap(),
//     )
//     .expect("Failed to lift gene");
// }

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Format {
    GTF,
    GFF3,
}

#[allow(clippy::too_many_arguments)]
fn lift_gene_helper(
    chain_path: &str,
    gff: &str,
    format: ArgFormat,
    output: &str,
    failed: &str,
) -> anyhow::Result<()> {
    let chain_file = PositionLiftOver::load(open(chain_path).context("Failed to open chain file")?)
        .context("Failed parse chain file.")?;
    let gene_lift = GeneLiftOver::new(chain_file);
    let mut writer = io::BufWriter::new(
        create(output, CompressionLevel::Default)
            .with_context(|| format!("Failed to create {}", output))?,
    );
    let mut failed_writer = io::BufWriter::new(
        create(failed, CompressionLevel::Default)
            .with_context(|| format!("Failed to create {}", failed))?,
    );

    let format = match format {
        ArgFormat::GFF3 => Format::GFF3,
        ArgFormat::GTF => Format::GTF,
        _ => {
            if gff.ends_with(".gtf") || gff.ends_with(".gtf.gz") {
                Format::GTF
            } else {
                Format::GFF3
            }
        }
    };

    match format {
        Format::GFF3 => {
            let mut reader =
                Gff3GroupedReader::new(Gff3Reader::new(io::BufReader::new(open(gff)?)));
            lift_gene_run(&gene_lift, &mut reader, &mut writer, &mut failed_writer)?;
        }
        Format::GTF => {
            let mut reader = GtfGroupedReader::new(GtfReader::new(io::BufReader::new(open(gff)?)));
            lift_gene_run(&gene_lift, &mut reader, &mut writer, &mut failed_writer)?;
        }
    }

    Ok(())
}

fn lift_gene_run<G: Feature + Display, T: Feature + Display, F: Feature + Display>(
    gene_lift: &GeneLiftOver,
    reader: &mut impl GroupedReader<G, T, F>,
    writer: &mut impl io::Write,
    failed_writer: &mut impl io::Write,
) -> Result<(), LiftOverError> {
    let mut processed_genes = 0;
    let mut processed_transcripts = 0;
    let mut full_succeeded_genes = 0;
    let mut partial_succeeded_genes = 0;
    let mut succeeded_transcripts = 0;

    for gene in reader {
        let gene = gene?;
        processed_genes += 1;
        processed_transcripts += gene.transcripts.len();
        match gene_lift.lift_gene_feature(&gene) {
            Ok(val) => {
                if val.failed_transcripts.is_empty() {
                    full_succeeded_genes += 1;
                } else {
                    partial_succeeded_genes += 1;
                }

                succeeded_transcripts += val.transcripts.len();

                write!(writer, "{}", val.apply())?;

                if let Some(failed_gene) = val.gene_with_failed_reason() {
                    write!(failed_writer, "{}", failed_gene)?;
                }
            }
            Err(e) => {
                let failed_gene = e.gene_with_failed_reason();
                write!(failed_writer, "{}", failed_gene)?;
            }
        }
    }

    eprintln!(
        "   Full Succeeded Genes: {} ({:.1}%)",
        full_succeeded_genes,
        f64::from(full_succeeded_genes) / f64::from(processed_genes) * 100f64
    );
    eprintln!(
        "Partial Succeeded Genes: {} ({:.1}%)",
        partial_succeeded_genes,
        f64::from(partial_succeeded_genes) / f64::from(processed_genes) * 100f64
    );
    eprintln!(
        "           Failed Genes: {} ({:.1}%)",
        processed_genes - partial_succeeded_genes - full_succeeded_genes,
        f64::from(processed_genes - full_succeeded_genes - partial_succeeded_genes)
            / f64::from(processed_genes)
            * 100f64
    );

    eprintln!(
        "  Succeeded Transcripts: {} ({:.1}%)",
        succeeded_transcripts,
        succeeded_transcripts as f64 / processed_transcripts as f64 * 100f64
    );
    eprintln!(
        "     Failed Transcripts: {} ({:.1}%)",
        processed_transcripts - succeeded_transcripts,
        (processed_transcripts - succeeded_transcripts) as f64 / processed_transcripts as f64
            * 100f64
    );

    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;
    use std::fs;

    #[test]
    fn test_lift_gff3() -> anyhow::Result<()> {
        fs::create_dir_all("../target/test-output/gene")?;

        lift_gene_helper(
            "../liftover-rs/testfiles/genomes/chain/GRCh38-to-GRCh37.chr22.chain",
            "../liftover-rs/testfiles/GENCODE/gencode.v33.basic.annotation.chr22.gff3.xz",
            ArgFormat::Auto,
            "../target/test-output/gene/gff-lift-gencode.v33.basic.annotation.chr22.mapped.gff3.gz",
            "../target/test-output/gene/gff-lift-gencode.v33.basic.annotation.chr22.failed.gff3.gz",
        )?;

        Ok(())
    }

    #[test]
    fn test_lift_gtf() -> anyhow::Result<()> {
        fs::create_dir_all("../target/test-output/gene")?;

        lift_gene_helper(
            "../liftover-rs/testfiles/genomes/chain/GRCh38-to-GRCh37.chr22.chain",
            "../liftover-rs/testfiles/GENCODE/gencode.v33.basic.annotation.chr22.gtf.xz",
            ArgFormat::GTF,
            "../target/test-output/gene/gff-lift-gencode.v33.annotation.chr22.mapped.gtf.gz",
            "../target/test-output/gene/gff-lift-gencode.v33.annotation.chr22.failed.gtf.gz",
        )?;

        Ok(())
    }

    #[test]
    fn test_lift_gff3_ensemble() -> anyhow::Result<()> {
        fs::create_dir_all("../target/test-output/gene")?;

        lift_gene_helper(
            "../liftover-rs/testfiles/genomes/chain/GRCh38-to-GRCh37.chr22.chain",
            "../liftover-rs/testfiles/GENCODE/Homo_sapiens.GRCh38.99.ensembl.chr22.gff3.xz",
            ArgFormat::Auto,
            "../target/test-output/gene/gff-lift-Homo_sapiens.GRCh38.99.chr22.mapped.gff3.gz",
            "../target/test-output/gene/gff-lift-Homo_sapiens.GRCh38.99.chr22.failed.gff3.gz",
        )?;

        Ok(())
    }

    #[test]
    fn test_lift_gtf_ensemble() -> anyhow::Result<()> {
        fs::create_dir_all("../target/test-output/gene")?;

        lift_gene_helper(
            "../liftover-rs/testfiles/genomes/chain/GRCh38-to-GRCh37.chr22.chain",
            "../liftover-rs/testfiles/GENCODE/Homo_sapiens.GRCh38.99.ensembl.chr22.gff3.xz",
            ArgFormat::Auto,
            "../target/test-output/gene/gff-lift-Homo_sapiens.GRCh38.99.chr22.mapped.gtf.gz",
            "../target/test-output/gene/gff-lift-Homo_sapiens.GRCh38.99.chr22.failed.gtf.gz",
        )?;

        Ok(())
    }
}
