use anyhow::{bail, Context, Result};
use memmap::MmapMut;
use std::fs::File;
use std::io::Write;
use std::path::Path;

pub struct Grid {
    _width: u32,
    _height: u32,
    cells_offset: usize,
    mmapped_data: MmapMut,
}

#[derive(Copy, Clone)]
pub struct CellData {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Grid {
    pub fn create_file(path: &Path, width: u32, height: u32) -> Result<()> {
        let mut file = File::create(path).context("Failed to create grid data file")?;
        file.write(b"Rplc")?; // magic
        file.write(&1u32.to_le_bytes())?; // version
        file.write(&width.to_le_bytes())?; // width
        file.write(&height.to_le_bytes())?; // height
        file.set_len(16 + 4u64 * (width as u64) * (height as u64))?; // zero-fill data
        Ok(())
    }

    pub fn create_file_with_data(
        path: &Path,
        width: u32,
        height: u32,
        data: &[Vec<CellData>],
    ) -> Result<()> {
        let mut file = File::create(path).context("Failed to create grid data file")?;
        file.write(b"Rplc")?; // magic
        file.write(&1u32.to_le_bytes())?; // version
        file.write(&width.to_le_bytes())?; // width
        file.write(&height.to_le_bytes())?; // height
        for y in 0..height {
            for x in 0..width {
                let cell = data[y as usize][x as usize];
                file.write(&[cell.r, cell.g, cell.b, cell.a])?;
            }
        }
        Ok(())
    }

    pub fn from_file(file: &File) -> Result<Grid> {
        let mmapped_data =
            unsafe { MmapMut::map_mut(file) }.context("Failed to mmap grid data file")?;

        if mmapped_data.len() < 8 {
            bail!("Grid data file is too small to contain a header");
        }

        if &mmapped_data[..4] != b"Rplc" {
            bail!("Grid data file does not contain a valid header");
        }

        let version = u32::from_le_bytes(mmapped_data[4..8].try_into().unwrap());
        match version {
            1 => {
                let width = u32::from_le_bytes(mmapped_data[8..12].try_into().unwrap());
                let height = u32::from_le_bytes(mmapped_data[12..16].try_into().unwrap());
                if mmapped_data.len() != 16 + 4usize * (width as usize) * (height as usize) {
                    bail!("Grid data file is of invalid size");
                }
                Ok(Grid {
                    _width: width,
                    _height: height,
                    cells_offset: 16,
                    mmapped_data,
                })
            }
            _ => bail!("Grid data file is of unknown version {}", version),
        }
    }

    pub fn width(&self) -> u32 {
        self._width
    }

    pub fn height(&self) -> u32 {
        self._height
    }

    pub fn get_cell(&self, x: usize, y: usize) -> Result<CellData> {
        if !(x < (self._width as usize) && y < (self._height as usize)) {
            bail!("Cell coordinates are out of bounds: X must be from 0 to {}, Y must be from 0 to {}, got X = {}, Y = {}", self._width - 1, self._height - 1, x, y);
        }
        let offset = self.cells_offset + (y * (self._width as usize) + x) * 4;
        let r = self.mmapped_data[offset];
        let g = self.mmapped_data[offset + 1];
        let b = self.mmapped_data[offset + 2];
        let a = self.mmapped_data[offset + 3];
        Ok(CellData { r, g, b, a })
    }

    pub fn set_cell(&mut self, x: usize, y: usize, value: CellData) -> Result<()> {
        if !(x < (self._width as usize) && y < (self._height as usize)) {
            bail!("Cell coordinates are out of bounds: X must be from 0 to {}, Y must be from 0 to {}, got X = {}, Y = {}", self._width - 1, self._height - 1, x, y);
        }
        let offset = self.cells_offset + (y * (self._width as usize) + x) * 4;
        self.mmapped_data[offset] = value.r;
        self.mmapped_data[offset + 1] = value.g;
        self.mmapped_data[offset + 2] = value.b;
        self.mmapped_data[offset + 3] = value.a;
        self.mmapped_data
            .flush_async()
            .context("Failed to flush grid data to disk")
    }

    pub fn get_data_serialized(&self) -> Vec<u8> {
        Vec::from(
            &self.mmapped_data[self.cells_offset
                ..self.cells_offset + (self._width as usize) * (self._height as usize) * 4],
        )
    }
}
