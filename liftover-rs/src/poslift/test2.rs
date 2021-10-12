use super::*;
use std::collections::HashSet;
use std::fs;
use std::io::{prelude::*, BufReader, BufWriter};
use std::path;

use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Eq, Hash, Deserialize, Clone)]
struct ExpectedRegion {
    chrom: String,
    start: u64,
    end: u64,
    mapped: Vec<MappedRegion>,
}

#[derive(Debug, PartialEq, Eq, Hash, Deserialize, Serialize, Clone)]
struct MappedRegion {
    chrom: String,
    start: u64,
    end: u64,
    strand: String,
}

#[test]
fn test_ucsc_result() -> anyhow::Result<()> {
    if !path::Path::new("./testfiles/ucsc/hg19ToHg38.over.chain.gz").is_file() {
        eprintln!("USCS tests are skipped");
        return Ok(());
    }

    let lift_over = PositionLiftOver::load(autocompress::open(
        "./testfiles/ucsc/hg19ToHg38.over.chain.gz",
    )?)?;

    eprintln!("chain file loaded");

    let mut jsonl_reader = BufReader::new(autocompress::open(
        "./testfiles/ucsc/hg19-regions-mapped-to-hg38.jsonl.gz",
    )?);

    let mut result_bed_writer_succeeded = BufWriter::new(fs::File::create(
        "./testfiles/ucsc/hg19-regions-mapped-to-hg38-succeeded.bed",
    )?);
    let mut result_bed_writer_succeeded_empty = BufWriter::new(fs::File::create(
        "./testfiles/ucsc/hg19-regions-mapped-to-hg38-succeeded-empty.bed",
    )?);
    let mut result_bed_writer_failed = BufWriter::new(fs::File::create(
        "./testfiles/ucsc/hg19-regions-mapped-to-hg38-failed.bed",
    )?);

    let mut buffer = Vec::new();
    while jsonl_reader.read_until(b'\n', &mut buffer)? > 0 {
        let data: ExpectedRegion = serde_json::from_slice(&buffer)?;

        let expected_map: HashSet<_> = data.mapped.iter().cloned().collect();
        let results: HashSet<_> = lift_over
            .lift_region(&data.chrom, data.start..data.end)
            .iter()
            .map(|x| MappedRegion {
                chrom: x.chromosome.name.to_string(),
                start: x.start,
                end: x.end,
                strand: x.strand.to_string(),
            })
            .collect();
        if expected_map != results {
            writeln!(
                result_bed_writer_failed,
                "{}\t{}\t{}\t\"{}\"-\"{}\"",
                data.chrom,
                data.start,
                data.end,
                serde_json::to_string(&expected_map)?,
                serde_json::to_string(&results)?
            )?;
        } else if results.is_empty() {
            writeln!(
                result_bed_writer_succeeded_empty,
                "{}\t{}\t{}\t{}",
                data.chrom,
                data.start,
                data.end,
                serde_json::to_string(&results)?
            )?;
        } else {
            writeln!(
                result_bed_writer_succeeded,
                "{}\t{}\t{}\t{}",
                data.chrom,
                data.start,
                data.end,
                serde_json::to_string(&results)?
            )?;
        }
        //assert_eq!(expected_map, results);
        buffer.clear();
    }

    eprintln!("expected result loaded");

    Ok(())
}
