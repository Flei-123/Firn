use std::hint::black_box;

const STEPS: i64 = 4_000_000;
const SOLAR_MASS: f64 = 4.0 * 3.141592653589793 * 3.141592653589793;
const DAYS: f64 = 365.24;

struct Bodies {
    px: [f64; 5],
    py: [f64; 5],
    pz: [f64; 5],
    vx: [f64; 5],
    vy: [f64; 5],
    vz: [f64; 5],
    ms: [f64; 5],
}

impl Bodies {
    fn set_body(&mut self, i: usize, x: f64, y: f64, z: f64, vx: f64, vy: f64, vz: f64, m: f64) {
        self.px[i] = x;
        self.py[i] = y;
        self.pz[i] = z;
        self.vx[i] = vx * DAYS;
        self.vy[i] = vy * DAYS;
        self.vz[i] = vz * DAYS;
        self.ms[i] = m * SOLAR_MASS;
    }

    fn init() -> Bodies {
        let mut b = Bodies { px: [0.0; 5], py: [0.0; 5], pz: [0.0; 5], vx: [0.0; 5], vy: [0.0; 5], vz: [0.0; 5], ms: [0.0; 5] };
        b.ms[0] = SOLAR_MASS;
        b.set_body(1, 4.84143144246472090e+00, -1.16032004402742839e+00, -1.03622044471123109e-01,
            1.66007664274403694e-03, 7.69901118419740425e-03, -6.90460016972063023e-05,
            9.54791938424326609e-04);
        b.set_body(2, 8.34336671824457987e+00, 4.12479856412430479e+00, -4.03523417114321381e-01,
            -2.76742510726862411e-03, 4.99852801234917238e-03, 2.30417297573763929e-05,
            2.85885980666130812e-04);
        b.set_body(3, 1.28943695621391310e+01, -1.51111514016986312e+01, -2.23307578892655734e-01,
            2.96460137564761618e-03, 2.37847173959480950e-03, -2.96589568540237556e-05,
            4.36624404335156298e-05);
        b.set_body(4, 1.53796971148509165e+01, -2.59193146099879641e+01, 1.79258772950371181e-01,
            2.68067772490389322e-03, 1.62824170038242295e-03, -9.51592254519715870e-05,
            5.15138902046611451e-05);
        let mut px = 0.0;
        let mut py = 0.0;
        let mut pz = 0.0;
        for i in 0..5 {
            px = px + b.vx[i] * b.ms[i];
            py = py + b.vy[i] * b.ms[i];
            pz = pz + b.vz[i] * b.ms[i];
        }
        b.vx[0] = 0.0 - px / SOLAR_MASS;
        b.vy[0] = 0.0 - py / SOLAR_MASS;
        b.vz[0] = 0.0 - pz / SOLAR_MASS;
        b
    }

    fn advance(&mut self, dt: f64) {
        for i in 0..5 {
            for j in (i + 1)..5 {
                let dx = self.px[i] - self.px[j];
                let dy = self.py[i] - self.py[j];
                let dz = self.pz[i] - self.pz[j];
                let d2 = dx * dx + dy * dy + dz * dz;
                let mag = dt / (d2 * d2.sqrt());
                self.vx[i] = self.vx[i] - dx * self.ms[j] * mag;
                self.vy[i] = self.vy[i] - dy * self.ms[j] * mag;
                self.vz[i] = self.vz[i] - dz * self.ms[j] * mag;
                self.vx[j] = self.vx[j] + dx * self.ms[i] * mag;
                self.vy[j] = self.vy[j] + dy * self.ms[i] * mag;
                self.vz[j] = self.vz[j] + dz * self.ms[i] * mag;
            }
        }
        for k in 0..5 {
            self.px[k] = self.px[k] + dt * self.vx[k];
            self.py[k] = self.py[k] + dt * self.vy[k];
            self.pz[k] = self.pz[k] + dt * self.vz[k];
        }
    }

    fn energy(&self) -> f64 {
        let mut e = 0.0;
        for i in 0..5 {
            e = e + 0.5 * self.ms[i] * (self.vx[i] * self.vx[i] + self.vy[i] * self.vy[i] + self.vz[i] * self.vz[i]);
            for j in (i + 1)..5 {
                let dx = self.px[i] - self.px[j];
                let dy = self.py[i] - self.py[j];
                let dz = self.pz[i] - self.pz[j];
                e = e - self.ms[i] * self.ms[j] / (dx * dx + dy * dy + dz * dz).sqrt();
            }
        }
        e
    }
}

fn main() {
    let mut b = Bodies::init();
    let mut s: i64 = 0;
    while s < black_box(STEPS) {
        b.advance(0.01);
        s += 1;
    }
    println!("{}", black_box((b.energy() * 1000000000.0) as i64));
}
