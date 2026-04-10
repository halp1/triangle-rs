use std::collections::HashMap;
use std::sync::LazyLock;

pub struct MatrixData {
  pub w: u32,
  pub h: u32,
  pub dx: i32,
  pub dy: i32,
  pub data: [Vec<(i32, i32, u8)>; 4],
}

pub struct PreviewData {
  pub w: u32,
  pub h: u32,
  pub data: Vec<(i32, i32, u8)>,
}

pub struct TetrominoEntry {
  pub matrix: MatrixData,
  pub preview: PreviewData,
  pub xweight: Option<u32>,
}

pub static TETROMINOES: LazyLock<HashMap<&'static str, TetrominoEntry>> = LazyLock::new(|| {
  let mut map = HashMap::new();

  map.insert(
    "i1",
    TetrominoEntry {
      matrix: MatrixData {
        w: 1,
        h: 1,
        dx: 0,
        dy: 1,
        data: [
          vec![(0, 0, 255)],
          vec![(0, 0, 255)],
          vec![(0, 0, 255)],
          vec![(0, 0, 255)],
        ],
      },
      preview: PreviewData {
        w: 1,
        h: 1,
        data: vec![(0, 0, 255)],
      },
      xweight: None,
    },
  );

  map.insert(
    "i2",
    TetrominoEntry {
      matrix: MatrixData {
        w: 2,
        h: 2,
        dx: 0,
        dy: 1,
        data: [
          vec![(0, 0, 199), (1, 0, 124)],
          vec![(1, 0, 241), (1, 1, 31)],
          vec![(1, 1, 124), (0, 1, 199)],
          vec![(0, 1, 31), (0, 0, 241)],
        ],
      },
      preview: PreviewData {
        w: 2,
        h: 1,
        data: vec![(0, 0, 199), (1, 0, 124)],
      },
      xweight: None,
    },
  );

  map.insert(
    "i3",
    TetrominoEntry {
      matrix: MatrixData {
        w: 3,
        h: 3,
        dx: 1,
        dy: 1,
        data: [
          vec![(0, 1, 199), (1, 1, 68), (2, 1, 124)],
          vec![(1, 0, 241), (1, 1, 17), (1, 2, 31)],
          vec![(2, 1, 124), (1, 1, 68), (0, 1, 199)],
          vec![(1, 2, 31), (1, 1, 17), (1, 0, 241)],
        ],
      },
      preview: PreviewData {
        w: 3,
        h: 1,
        data: vec![(0, 0, 199), (1, 0, 68), (2, 0, 124)],
      },
      xweight: None,
    },
  );

  map.insert(
    "l3",
    TetrominoEntry {
      matrix: MatrixData {
        w: 2,
        h: 2,
        dx: 0,
        dy: 1,
        data: [
          vec![(0, 0, 241), (0, 1, 39), (1, 1, 124)],
          vec![(1, 0, 124), (0, 0, 201), (0, 1, 31)],
          vec![(1, 1, 31), (1, 0, 114), (0, 0, 199)],
          vec![(0, 1, 199), (1, 1, 156), (1, 0, 241)],
        ],
      },
      preview: PreviewData {
        w: 2,
        h: 2,
        data: vec![(0, 0, 241), (0, 1, 39), (1, 1, 124)],
      },
      xweight: None,
    },
  );

  map.insert(
    "i5",
    TetrominoEntry {
      matrix: MatrixData {
        w: 5,
        h: 5,
        dx: 2,
        dy: 2,
        data: [
          vec![(0, 2, 199), (1, 2, 68), (2, 2, 68), (3, 2, 68), (4, 2, 124)],
          vec![(2, 0, 241), (2, 1, 17), (2, 2, 17), (2, 3, 17), (2, 4, 31)],
          vec![(4, 2, 124), (3, 2, 68), (2, 2, 68), (1, 2, 68), (0, 2, 199)],
          vec![(2, 4, 31), (2, 3, 17), (2, 2, 17), (2, 1, 17), (2, 0, 241)],
        ],
      },
      preview: PreviewData {
        w: 5,
        h: 1,
        data: vec![(0, 0, 199), (1, 0, 68), (2, 0, 68), (3, 0, 68), (4, 0, 124)],
      },
      xweight: None,
    },
  );

  map.insert(
    "z",
    TetrominoEntry {
      matrix: MatrixData {
        w: 3,
        h: 3,
        dx: 1,
        dy: 1,
        data: [
          vec![(0, 0, 199), (1, 0, 114), (1, 1, 39), (2, 1, 124)],
          vec![(2, 0, 241), (2, 1, 156), (1, 1, 201), (1, 2, 31)],
          vec![(2, 2, 124), (1, 2, 39), (1, 1, 114), (0, 1, 199)],
          vec![(0, 2, 31), (0, 1, 201), (1, 1, 156), (1, 0, 241)],
        ],
      },
      preview: PreviewData {
        w: 3,
        h: 2,
        data: vec![(0, 0, 199), (1, 0, 114), (1, 1, 39), (2, 1, 124)],
      },
      xweight: None,
    },
  );

  map.insert(
    "l",
    TetrominoEntry {
      matrix: MatrixData {
        w: 3,
        h: 3,
        dx: 1,
        dy: 1,
        data: [
          vec![(2, 0, 241), (0, 1, 199), (1, 1, 68), (2, 1, 156)],
          vec![(2, 2, 124), (1, 0, 241), (1, 1, 17), (1, 2, 39)],
          vec![(0, 2, 31), (2, 1, 124), (1, 1, 68), (0, 1, 201)],
          vec![(0, 0, 199), (1, 2, 31), (1, 1, 17), (1, 0, 114)],
        ],
      },
      preview: PreviewData {
        w: 3,
        h: 2,
        data: vec![(2, 0, 241), (0, 1, 199), (1, 1, 68), (2, 1, 156)],
      },
      xweight: None,
    },
  );

  map.insert(
    "o",
    TetrominoEntry {
      matrix: MatrixData {
        w: 2,
        h: 2,
        dx: 0,
        dy: 1,
        data: [
          vec![(0, 0, 193), (1, 0, 112), (0, 1, 7), (1, 1, 28)],
          vec![(1, 0, 112), (1, 1, 28), (0, 0, 193), (0, 1, 7)],
          vec![(1, 1, 28), (0, 1, 7), (1, 0, 112), (0, 0, 193)],
          vec![(0, 1, 7), (0, 0, 193), (1, 1, 28), (1, 0, 112)],
        ],
      },
      preview: PreviewData {
        w: 2,
        h: 2,
        data: vec![(0, 0, 193), (1, 0, 112), (0, 1, 7), (1, 1, 28)],
      },
      xweight: None,
    },
  );

  map.insert(
    "s",
    TetrominoEntry {
      matrix: MatrixData {
        w: 3,
        h: 3,
        dx: 1,
        dy: 1,
        data: [
          vec![(1, 0, 201), (2, 0, 124), (0, 1, 199), (1, 1, 156)],
          vec![(2, 1, 114), (2, 2, 31), (1, 0, 241), (1, 1, 39)],
          vec![(1, 2, 156), (0, 2, 199), (2, 1, 124), (1, 1, 201)],
          vec![(0, 1, 39), (0, 0, 241), (1, 2, 31), (1, 1, 114)],
        ],
      },
      preview: PreviewData {
        w: 3,
        h: 2,
        data: vec![(1, 0, 201), (2, 0, 124), (0, 1, 199), (1, 1, 156)],
      },
      xweight: None,
    },
  );

  map.insert(
    "i",
    TetrominoEntry {
      matrix: MatrixData {
        w: 4,
        h: 4,
        dx: 1,
        dy: 1,
        data: [
          vec![(0, 1, 199), (1, 1, 68), (2, 1, 68), (3, 1, 124)],
          vec![(2, 0, 241), (2, 1, 17), (2, 2, 17), (2, 3, 31)],
          vec![(3, 2, 124), (2, 2, 68), (1, 2, 68), (0, 2, 199)],
          vec![(1, 3, 31), (1, 2, 17), (1, 1, 17), (1, 0, 241)],
        ],
      },
      preview: PreviewData {
        w: 4,
        h: 1,
        data: vec![(0, 0, 199), (1, 0, 68), (2, 0, 68), (3, 0, 124)],
      },
      xweight: None,
    },
  );

  map.insert(
    "j",
    TetrominoEntry {
      matrix: MatrixData {
        w: 3,
        h: 3,
        dx: 1,
        dy: 1,
        data: [
          vec![(0, 0, 241), (0, 1, 39), (1, 1, 68), (2, 1, 124)],
          vec![(2, 0, 124), (1, 0, 201), (1, 1, 17), (1, 2, 31)],
          vec![(2, 2, 31), (2, 1, 114), (1, 1, 68), (0, 1, 199)],
          vec![(0, 2, 199), (1, 2, 156), (1, 1, 17), (1, 0, 241)],
        ],
      },
      preview: PreviewData {
        w: 3,
        h: 2,
        data: vec![(0, 0, 241), (0, 1, 39), (1, 1, 68), (2, 1, 124)],
      },
      xweight: None,
    },
  );

  map.insert(
    "t",
    TetrominoEntry {
      matrix: MatrixData {
        w: 3,
        h: 3,
        dx: 1,
        dy: 1,
        data: [
          vec![(1, 0, 241), (0, 1, 199), (1, 1, 164), (2, 1, 124)],
          vec![(2, 1, 124), (1, 0, 241), (1, 1, 41), (1, 2, 31)],
          vec![(1, 2, 31), (2, 1, 124), (1, 1, 74), (0, 1, 199)],
          vec![(0, 1, 199), (1, 2, 31), (1, 1, 146), (1, 0, 241)],
        ],
      },
      preview: PreviewData {
        w: 3,
        h: 2,
        data: vec![(1, 0, 241), (0, 1, 199), (1, 1, 164), (2, 1, 124)],
      },
      xweight: None,
    },
  );

  map.insert(
    "oo",
    TetrominoEntry {
      matrix: MatrixData {
        w: 4,
        h: 4,
        dx: 1,
        dy: 1,
        data: [
          vec![
            (0, 1, 193),
            (1, 1, 64),
            (2, 1, 64),
            (3, 1, 112),
            (0, 2, 7),
            (1, 2, 4),
            (2, 2, 4),
            (3, 2, 28),
          ],
          vec![
            (2, 0, 112),
            (2, 1, 16),
            (2, 2, 16),
            (2, 3, 28),
            (1, 0, 193),
            (1, 1, 1),
            (1, 2, 1),
            (1, 3, 7),
          ],
          vec![
            (3, 2, 28),
            (2, 2, 68),
            (1, 2, 68),
            (0, 2, 7),
            (3, 1, 112),
            (2, 1, 64),
            (1, 1, 64),
            (0, 1, 193),
          ],
          vec![
            (1, 3, 7),
            (1, 2, 1),
            (1, 1, 1),
            (1, 0, 193),
            (2, 3, 28),
            (2, 2, 16),
            (2, 1, 16),
            (2, 0, 112),
          ],
        ],
      },
      preview: PreviewData {
        w: 4,
        h: 2,
        data: vec![
          (0, 0, 193),
          (1, 0, 64),
          (2, 0, 64),
          (3, 0, 112),
          (0, 1, 7),
          (1, 1, 4),
          (2, 1, 4),
          (3, 1, 28),
        ],
      },
      xweight: Some(1),
    },
  );

  map
});
