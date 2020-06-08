use bbqueue::{consts::*, GenericBBBuffer};
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::cmp::min;

const DATA_SZ: usize = 128 * 1024 * 1024;

pub fn criterion_benchmark(c: &mut Criterion) {
    let data = vec![0; DATA_SZ].into_boxed_slice();

    c.bench_function("bbq 128/4096", |bench| bench.iter(|| chunky(&data, 128)));

    c.bench_function("bbq 256/4096", |bench| bench.iter(|| chunky(&data, 256)));

    c.bench_function("bbq 512/4096", |bench| bench.iter(|| chunky(&data, 512)));

    c.bench_function("bbq 1024/4096", |bench| bench.iter(|| chunky(&data, 1024)));

    c.bench_function("bbq 2048/4096", |bench| bench.iter(|| chunky(&data, 2048)));

    let buffy: GenericBBBuffer<u8, U65536> = GenericBBBuffer::new();
    let (mut prod, mut cons) = buffy.try_split().unwrap();

    c.bench_function("bbq 8192/65536", |bench| {
        let chunksz = 8192;

        bench.iter(|| {
            black_box(thread::scope(|sc| {
                sc.spawn(|_| {
                    data.chunks(chunksz).for_each(|ch| loop {
                        if let Ok(mut wgr) = prod.grant_exact(chunksz) {
                            wgr.copy_from_slice(black_box(ch));
                            wgr.commit(chunksz);
                            break;
                        }
                    });
                });

                sc.spawn(|_| {
                    data.chunks(chunksz).for_each(|ch| {
                        let mut st = 0;
                        loop {
                            if let Ok(rgr) = cons.read() {
                                let len = min(chunksz - st, rgr.len());
                                assert_eq!(ch[st..st + len], rgr[..len]);
                                rgr.release(len);

                                st += len;

                                if st == chunksz {
                                    break;
                                }
                            }
                        }
                    });
                });
            }))
            .unwrap();
        })
    });

    use std::mem::MaybeUninit;

    c.bench_function("std channels 8192 unbounded", |bench| {
        bench.iter(|| {
            use std::sync::mpsc::{Receiver, Sender};
            let (mut prod, mut cons): (Sender<[u8; 8192]>, Receiver<[u8; 8192]>) =
                std::sync::mpsc::channel();
            let rdata = &data;

            thread::scope(|sc| {
                sc.spawn(move |_| {
                    rdata.chunks(8192).for_each(|ch| {
                        let mut x: MaybeUninit<[u8; 8192]> = MaybeUninit::uninit();
                        unsafe {
                            x.as_mut_ptr()
                                .copy_from_nonoverlapping(ch.as_ptr().cast::<[u8; 8192]>(), 1)
                        };
                        prod.send(unsafe { x.assume_init() }).unwrap();
                    });
                });

                sc.spawn(move |_| {
                    rdata.chunks(8192).for_each(|ch| {
                        let x = cons.recv().unwrap();
                        assert_eq!(&x[..], &ch[..]);
                    });
                });
            })
            .unwrap();
        })
    });

    c.bench_function("xbeam channels 8192/65536", |bench| {
        bench.iter(|| {
            use crossbeam::{bounded, Receiver, Sender};
            let (mut prod, mut cons): (Sender<[u8; 8192]>, Receiver<[u8; 8192]>) =
                bounded(65536 / 8192);
            let rdata = &data;

            thread::scope(|sc| {
                sc.spawn(move |_| {
                    rdata.chunks(8192).for_each(|ch| {
                        let mut x: MaybeUninit<[u8; 8192]> = MaybeUninit::uninit();
                        unsafe {
                            x.as_mut_ptr()
                                .copy_from_nonoverlapping(ch.as_ptr().cast::<[u8; 8192]>(), 1)
                        };
                        prod.send(unsafe { x.assume_init() }).unwrap();
                    });
                });

                sc.spawn(move |_| {
                    rdata.chunks(8192).for_each(|ch| {
                        let x = cons.recv().unwrap();
                        assert_eq!(&x[..], &ch[..]);
                    });
                });
            })
            .unwrap();
        })
    });

    cfg_if::cfg_if! {
        if #[cfg(feature = "nightly")] {
            c.bench_function("bounded queue 8192/65536", |bench| {

                bench.iter(|| {
                    use bounded_spsc_queue::make;
                    let (mut prod, mut cons) = make::<[u8; 8192]>(65536 / 8192);
                    let rdata = &data;

                    thread::scope(|sc| {
                        sc.spawn(move |_| {
                            rdata.chunks(8192).for_each(|ch| {
                                let mut x: MaybeUninit<[u8; 8192]> = MaybeUninit::uninit();
                                unsafe {
                                    x.as_mut_ptr().copy_from_nonoverlapping(ch.as_ptr().cast::<[u8; 8192]>(), 1)
                                };
                                prod.push(unsafe { x.assume_init() });
                            });
                        });

                        sc.spawn(move |_| {
                            rdata.chunks(8192).for_each(|ch| {
                                let x = cons.pop();
                                assert_eq!(&x[..], &ch[..]);
                            });
                        });
                    }).unwrap();
                })
            });
        }
    }

    use heapless::spsc::Queue;

    let mut queue: Queue<[u8; 8192], U8> = Queue::new();
    let (mut prod, mut cons) = queue.split();

    c.bench_function("heapless spsc::Queue 8192/65536", |bench| {
        let chunksz = 8192;

        bench.iter(|| {
            black_box(thread::scope(|sc| {
                sc.spawn(|_| {
                    data.chunks(chunksz).for_each(|ch| {
                        let mut x: MaybeUninit<[u8; 8192]> = MaybeUninit::uninit();
                        unsafe {
                            x.as_mut_ptr()
                                .copy_from_nonoverlapping(ch.as_ptr().cast::<[u8; 8192]>(), 1)
                        };
                        let mut x = unsafe { x.assume_init() };

                        loop {
                            match prod.enqueue(x) {
                                Ok(_) => break,
                                Err(y) => x = y,
                            };
                        }
                    });
                });

                sc.spawn(|_| {
                    data.chunks(8192).for_each(|ch| loop {
                        if let Some(x) = cons.dequeue() {
                            assert_eq!(&x[..], &ch[..]);
                            break;
                        }
                    });
                });
            }))
            .unwrap();
        })
    });
}

use crossbeam_utils::thread;
fn chunky(data: &[u8], chunksz: usize) {
    let buffy: GenericBBBuffer<u8, U4096> = GenericBBBuffer::new();
    let (mut prod, mut cons) = buffy.try_split().unwrap();

    thread::scope(|sc| {
        let pjh = sc.spawn(|_| {
            data.chunks(chunksz).for_each(|ch| loop {
                if let Ok(mut wgr) = prod.grant_exact(chunksz) {
                    wgr.copy_from_slice(ch);
                    wgr.commit(chunksz);
                    break;
                }
            });
        });

        let cjh = sc.spawn(|_| {
            data.chunks(chunksz).for_each(|ch| {
                let mut st = 0;
                loop {
                    if let Ok(rgr) = cons.read() {
                        let len = min(chunksz - st, rgr.len());
                        assert_eq!(ch[st..st + len], rgr[..len]);
                        rgr.release(len);

                        st += len;

                        if st == chunksz {
                            break;
                        }
                    }
                }
            });
        });
    })
    .unwrap();
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
