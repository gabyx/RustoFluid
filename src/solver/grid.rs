use crate::log::{debug, warn, Logger};
use crate::solver::timestepper::Integrate;
use crate::types::*;
use std::num::Wrapping;

#[derive(Clone, Debug, PartialEq)]
pub enum CellTypes {
    Solid,
    Fluid,
}

#[derive(Clone, Debug)]
pub struct Cell {
    // Velocity x,y:
    // - v_x is at the location (h/2, 0),
    // - v_y is at the location (0, h/2),
    pub velocity: FrontBackBuffer<Vector2>,

    pub pressure: Scalar,
    pub smoke: FrontBackBuffer<Scalar>,

    pub mode: CellTypes,

    index: Index2,
}

impl Cell {
    pub fn new(index: Index2) -> Self {
        let default_vel = Vector2::from_element(0.0);
        let default_pressure = 0.0;
        let default_smoke = 0.0;

        return Cell {
            velocity: FrontBackBuffer {
                front: default_vel,
                back: default_vel,
            },
            pressure: default_pressure,
            smoke: FrontBackBuffer {
                front: default_smoke,
                back: default_smoke,
            },
            mode: CellTypes::Fluid,
            index,
        };
    }

    pub fn index(&self) -> Index2 {
        return self.index;
    }
}

pub struct Grid {
    pub cell_width: Scalar,
    pub dim: Index2,

    cells: Vec<Cell>,

    extent: Vector2,

    // Grid offsets for each axis of the velocity in the cells..
    offsets: [Vector2; 2],
}

#[derive(Copy, Clone, Debug)]
pub struct GridIndex {
    pub index: Index2,
    dim: Index2,
}

pub struct GridIndexIterator {
    curr: GridIndex,

    min: Index2,
    max: Index2,
}

impl GridIndex {
    fn to_data_index(&self) -> usize {
        return self.index.x + self.dim.x * self.index.y;
    }
}

impl Iterator for GridIndexIterator {
    type Item = GridIndex;

    fn next(&mut self) -> Option<Self::Item> {
        let curr = self.curr; // Copy current.

        // Advance to next cell.
        let next = &mut self.curr;
        next.index.x += 1;
        if next.index.x >= self.max.x {
            next.index.y += 1;
            next.index.x = self.min.x;
        }

        if Grid::is_inside_range(self.min, self.max, curr.index) {
            return Some(curr);
        }

        return None;
    }
}

impl Grid {
    pub fn new(mut dim_x: usize, mut dim_y: usize, cell_width: Scalar) -> Self {
        dim_x += 2;
        dim_y += 2;
        let n = dim_x * dim_y;

        let h_2 = cell_width as Scalar * 0.5;
        let dim = Index2::new(dim_x, dim_y);
        let extent = dim.cast::<Scalar>() * cell_width;

        let mut grid = Grid {
            cell_width,
            dim: Index2::new(dim_x, dim_y),
            cells: vec![Cell::new(Index2::new(0, 0)); n],

            extent,
            // `x`-values lie at offest `(0, h/2)` and `y`-values at `(h/2, 0)`.
            offsets: [Vector2::new(0.0, h_2), Vector2::new(h_2, 0.0)],
        };

        // Setup grid.
        for it in grid.to_index_iter() {
            let mode = if Grid::is_inside_border(grid.dim, it.index) {
                CellTypes::Fluid
            } else {
                CellTypes::Solid
            };

            let mut cell = Cell::new(it.index);
            cell.mode = mode;

            grid.cells[it.to_data_index()] = cell;
        }

        return grid;
    }

    fn to_index_iter(&self) -> GridIndexIterator {
        return GridIndexIterator {
            curr: GridIndex {
                index: Index2::new(0, 0),
                dim: self.dim,
            },
            min: Index2::new(0, 0),
            max: self.dim,
        };
    }

    fn to_inside_index_iter(&self) -> GridIndexIterator {
        return GridIndexIterator {
            curr: GridIndex {
                index: Index2::new(1, 1),
                dim: self.dim,
            },
            min: Index2::new(1, 1),
            max: self.dim - Index2::new(1, 1),
        };
    }

    pub fn clamp_to_range<T>(min: Vector2T<T>, max: Vector2T<T>, index: Vector2T<T>) -> Vector2T<T>
    where
        T: nalgebra::Scalar + PartialOrd + Copy,
    {
        return Vector2T::<T>::new(
            nalgebra::clamp(index.x, min.x, max.x),
            nalgebra::clamp(index.y, min.y, max.y),
        );
    }

    pub fn is_inside_range(min: Index2, max: Index2, index: Index2) -> bool {
        return index < max && index >= min;
    }

    fn is_inside_border(dim: Index2, index: Index2) -> bool {
        return index > Index2::zeros() && index < (dim - Index2::new(1, 1));
    }

    fn get_neighbors_indices(index: Index2) -> [[Index2; 2]; 2] {
        let decrement = |x| (Wrapping(x) - Wrapping(1usize)).0;

        return [
            [
                // Negative neighbors.
                Index2::new(decrement(index.x), index.y),
                Index2::new(index.x, decrement(index.y)),
            ],
            [
                // Positive neighbors.
                Index2::new(index.x + 1, index.y),
                Index2::new(index.x, index.y + 1),
            ],
        ];
    }
}

pub trait CellGetter<'a, I> {
    type Item: 'a;

    type Output = &'a Self::Item;
    type OutputMut = &'a mut Self::Item;

    fn cell(&'a self, index: I) -> Self::Output;
    fn cell_mut(&'a mut self, index: I) -> Self::OutputMut;

    type OutputOpt = Option<&'a Self::Item>;
    type OutputMutOpt = Option<&'a mut Self::Item>;

    fn cell_opt(&'a self, index: Index2) -> Self::OutputOpt;
    fn cell_mut_opt(&'a mut self, index: Index2) -> Self::OutputMutOpt;
}

impl<'t> CellGetter<'t, Index2> for Grid {
    type Item = Cell;

    fn cell(&'t self, index: Index2) -> &Cell {
        return &self.cells[index.x + index.y * self.dim.x];
    }

    fn cell_mut(&'t mut self, index: Index2) -> &mut Cell {
        return &mut self.cells[index.x + index.y * self.dim.x];
    }

    fn cell_opt(&'t self, index: Index2) -> Option<&Cell> {
        return Grid::is_inside_range(Index2::zeros(), self.dim, index).then(|| self.cell(index));
    }

    fn cell_mut_opt(&'t mut self, index: Index2) -> Option<&mut Cell> {
        return Grid::is_inside_range(Index2::zeros(), self.dim, index)
            .then(|| self.cell_mut(index));
    }
}

impl Grid {
    pub fn modify_cells<F, const N: usize>(&mut self, indices: [usize; N], mut f: F) -> ()
    where
        F: FnMut([&mut Cell; N]),
    {
        let refs = self.cells.get_many_mut(indices).expect("Wrong indices.");
        f(refs);
    }
}

impl Integrate for Cell {
    fn integrate(&mut self, _log: &Logger, dt: Scalar, gravity: Vector2) {
        self.velocity.front = self.velocity.back + dt * gravity;
    }
}

impl Integrate for Grid {
    fn integrate(&mut self, log: &Logger, dt: Scalar, gravity: Vector2) {
        debug!(log, "Integrate grid.");

        for cell in self.cells.iter_mut() {
            cell.integrate(log, dt, gravity); // integrate
        }

        self.enforce_solid_constraints(log);
    }

    fn solve_incompressibility(
        &mut self,
        log: &Logger,
        dt: Scalar,
        iterations: u64,
        density: Scalar,
    ) {
        let r = 1.9; // Overrelaxation factor.

        let cp = density * self.cell_width / dt;

        for _iter in 0..iterations {
            for it in self.to_inside_index_iter() {
                let index = it.index;
                let dim = self.dim;

                assert!(
                    Grid::is_inside_border(dim, index),
                    "Index {} is not inside",
                    index
                );

                if self.cell(index).mode == CellTypes::Solid {
                    continue;
                }

                let s_factor = |index: Index2| {
                    return if self.cell(index).mode == CellTypes::Solid {
                        0.0
                    } else {
                        1.0
                    };
                };

                let nbs = Grid::get_neighbors_indices(index);

                // Normalization values `s`
                // for negative/positive neighbors.
                // - 0: solid, 1: fluid.
                let mut nbs_s = [Vector2::zeros(), Vector2::zeros()];
                let mut s = 0.0;

                for dir in 0..2 {
                    nbs_s[dir] = Vector2::new(s_factor(nbs[dir][0]), s_factor(nbs[dir][1]));
                    s += nbs_s[dir].sum();
                }

                if s == 0.0 {
                    warn!(log, "Fluid in-face count is 0.0 for {:?}", index);
                    continue;
                }

                let get_vel = |index: Index2, dir: usize| {
                    return self.cell(index).velocity.front[dir];
                };

                let mut div: Scalar = 0.0; // Net outflow on this cell.
                let pos_idx = 1usize;
                let nbs_pos = &nbs[pos_idx];
                for xy in 0..2 {
                    div += get_vel(nbs_pos[xy], xy) - get_vel(index, xy)
                }

                // Normalize outflow to the cells we can control.
                let p = div / s;
                self.cell_mut(index).pressure -= cp * p;

                // Add outflow-part to inflows to reach net 0-outflow.
                self.cell_mut(index).velocity.front += r * nbs_s[0] * p;

                // Subtract outflow-part to outflows to reach net 0-outflow.
                self.cell_mut(nbs[pos_idx][0]).velocity.front.x -= r * nbs_s[pos_idx].x * p;
                self.cell_mut(nbs[pos_idx][1]).velocity.front.y -= r * nbs_s[pos_idx].y * p;
            }
        }

        for it in self.to_index_iter() {
            self.cell_mut(it.index).velocity.swap();
        }
    }
}

impl Grid {
    pub fn sample_field<F: Fn(&Cell, usize) -> Scalar>(
        &self,
        mut pos: Vector2,
        dir: usize,
        get_val: F,
    ) -> Scalar {
        let h = self.cell_width;
        let h_inv = 1.0 / self.cell_width;
        let h_2 = 0.5 * h;

        let offset = self.offsets[dir];
        pos = pos - offset; // Compute position on staggered grid.
        pos = Grid::clamp_to_range(Vector2::zeros(), self.extent, pos);

        // Compute index.
        let mut index = Index2::from_iterator((pos * h_inv).iter().map(|v| *v as usize));

        let clamp_index = |i| Grid::clamp_to_range(Index2::zeros(), self.dim, i);

        index = clamp_index(index);
        let pos_cell = pos - index.cast::<Scalar>() * h;
        let alpha = pos_cell * h_inv;

        // Get all neighbor indices.
        // [ (1,0), (1,1)
        //   (0,0), (1,1) ]
        let nbs = [
            clamp_index(index + Index2::new(1, 0)),
            clamp_index(index + Index2::new(1, 1)),
            index,
            clamp_index(index + Index2::new(0, 1)),
        ];

        // Get all values on the grid.
        let values = Matrix2::from_iterator(nbs.map(|i| get_val(self.cell(i), dir)).into_iter());

        let f1 = values * Vector2::new(1.0 - alpha.y, alpha.y);

        return Vector2::new(alpha.x, 1.0 - alpha.x).dot(&f1);
    }

    fn enforce_solid_constraints(&mut self, log: &Logger) {
        debug!(log, "Enforce solid constraints on solid cells.");

        // Enforce solid constraint over all cells which are solid.
        for it in self.to_index_iter() {
            let index = it.index;

            {
                let cell = self.cell_mut(index);
                if cell.mode != CellTypes::Solid {
                    continue;
                }

                // Cell is solid, so constrain all involved staggered velocity.
                // to the last one and also for the neighbors in x and y direction.
                cell.velocity.front = cell.velocity.back;
            }

            for idx in 0..2usize {
                let mut nb_index = index;

                match idx {
                    0 => nb_index.x += 1, // x neighbor.
                    1 => nb_index.y += 1, // y neighbor.
                    _ => {}
                }

                let cell = self.cell_mut_opt(nb_index);
                match cell {
                    Some(c) => {
                        c.velocity.front[idx] = c.velocity.back[idx]; // reset only the x,y direction.
                    }
                    None => {}
                }
            }
        }
    }
}
