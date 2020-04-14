#[cfg(test)]
mod tests {

    use bbqueue::{
        ConstBBBuffer, BBBuffer, consts::*, Producer, Consumer, GrantR, GrantW,
        ArrayLength,
    };

    enum Potato<'a, N>
    where
        N: ArrayLength<u8>,
    {
        Tx((Producer<'a, N>, u8)),
        Rx((Consumer<'a, N>, u8)),
        TxG(GrantW<'a, N>),
        RxG(GrantR<'a, N>),
        Idle,
        Done,
    }

    impl<'a, N> Potato<'a, N>
    where
        N: ArrayLength<u8>
    {
        fn work(self) -> (Self, Self) {
            match self {
                Self::Tx((mut prod, ct)) => {
                    // If we are holding a producer, try to send three things before passing it on.
                    if ct == 0 {
                        // If we have exhausted our counts, pass on the sender.
                        (Self::Idle, Self::Tx((prod, 3)))
                    } else {
                        // If we get a grant, pass it on, otherwise keep trying
                        if let Ok(wgr) = prod.grant_exact(17) {
                            (Self::Tx((prod, ct - 1)), Self::TxG(wgr))
                        } else {
                            (Self::Tx((prod, ct)), Self::Idle)
                        }
                    }
                }
                Self::Rx((mut cons, ct)) => {
                    // If we are holding a consumer, try to send three things before passing it on.
                    if ct == 0 {
                        // If we have exhausted our counts, pass on the sender.
                        (Self::Idle, Self::Rx((cons, 3)))
                    } else {
                        // If we get a grant, pass it on, otherwise keep trying
                        if let Ok(rgr) = cons.read() {
                            (Self::Rx((cons, ct - 1)), Self::RxG(rgr))
                        } else {
                            (Self::Rx((cons, ct)), Self::Idle)
                        }
                    }
                }
                Self::TxG(mut gr_w) => {
                    gr_w.iter_mut().take(17).enumerate().for_each(|(i, by)| *by = i as u8);
                    gr_w.commit(17);
                    (Self::Idle, Self::Idle)
                }
                Self::RxG(gr_r) => {
                    gr_r.iter().take(17).enumerate().for_each(|(i, by)| assert_eq!(*by, i as u8));
                    gr_r.release(17);
                    (Self::Idle, Self::Idle)
                }
                Self::Idle => {
                    #[allow(deprecated)]
                    std::thread::sleep_ms(1);
                    (Self::Idle, Self::Idle)
                }
                Self::Done => {
                    (Self::Idle, Self::Done)
                }
            }
        }
    }

    static BB: BBBuffer<U4096> = BBBuffer( ConstBBBuffer::new() );

    use std::sync::mpsc::{channel, Sender, Receiver};
    use std::thread::spawn;


    #[test]
    fn hello() {
        let (prod, cons) = BB.try_split().unwrap();

        // create the channels
        let (mut tx_1_2, rx_1_2): (Sender<Potato<'static, U4096>>, Receiver<Potato<'static, U4096>>) = channel();
        let (tx_2_3, rx_2_3): (Sender<Potato<'static, U4096>>, Receiver<Potato<'static, U4096>>) = channel();
        let (tx_3_4, rx_3_4): (Sender<Potato<'static, U4096>>, Receiver<Potato<'static, U4096>>) = channel();
        let (tx_4_1, rx_4_1): (Sender<Potato<'static, U4096>>, Receiver<Potato<'static, U4096>>) = channel();

        tx_1_2.send(Potato::Tx((prod, 3))).unwrap();
        tx_1_2.send(Potato::Rx((cons, 3))).unwrap();

        let thread_1 = spawn(move || {
            let mut count = 1_000_000;
            let mut me: Potato<'static, U4096> = Potato::Idle;

            loop {
                if let Potato::Idle = me {
                    if let Ok(new) = rx_4_1.recv() {
                        if let Potato::Tx(tx) = new {
                            count -= 1;
                            if count == 0 {
                                me = Potato::Done;
                            } else {
                                me = Potato::Tx(tx);
                            }
                        } else {
                            me = new;
                        }
                    } else {
                        continue;
                    }
                }
                let (new_me, send) = me.work();

                let we_done = if let Potato::Done = &send {
                    true
                } else {
                    false
                };

                let nop = if let Potato::Idle = &send {
                    true
                } else {
                    false
                };

                if !nop {
                    tx_1_2.send(send).unwrap();
                }

                if we_done {
                    return;
                }

                me = new_me;
            }

        });

        let closure_2_3_4 = move |rx: Receiver<Potato<'static, U4096>>, tx: Sender<Potato<'static, U4096>>| {
            let mut me: Potato<'static, U4096> = Potato::Idle;

            loop {
                if let Potato::Idle = me {
                    if let Ok(new) = rx.try_recv() {
                        me = new;
                    } else {
                        continue;
                    }
                }
                let (new_me, send) = me.work();

                let we_done = if let Potato::Done = &send {
                    true
                } else {
                    false
                };

                let nop = if let Potato::Idle = &send {
                    true
                } else {
                    false
                };

                if !nop {
                    tx.send(send).ok();
                }

                if we_done {
                    return;
                }

                me = new_me;
            }

        };

        let thread_2 = spawn(move || closure_2_3_4(rx_1_2, tx_2_3));
        let thread_3 = spawn(move || closure_2_3_4(rx_2_3, tx_3_4));
        let thread_4 = spawn(move || closure_2_3_4(rx_3_4, tx_4_1));

        thread_1.join().unwrap();
        thread_2.join().unwrap();
        thread_3.join().unwrap();
        thread_4.join().unwrap();
    }

}
