use bbqueue::{
    consts, framed::FrameConsumer, framed::FrameGrantR, framed::FrameProducer, BBBuffer,
    ConstBBBuffer,
};
use postcard::{from_bytes, to_slice};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

static BB: BBBuffer<consts::U1024> = BBBuffer(ConstBBBuffer::new());

struct Txer {
    prod: FrameProducer<'static, consts::U1024>,
}

impl Txer {
    fn send<T>(&mut self, msg: &T) -> Result<(), ()>
    where
        T: Serialize,
    {
        let mut wgr = self.prod.grant(128).map_err(drop)?;
        let used = to_slice(msg, &mut wgr).map_err(drop)?.len();
        wgr.commit(used);
        Ok(())
    }
}

struct Rxer {
    cons: FrameConsumer<'static, consts::U1024>,
}

struct Message<'a> {
    fgr: FrameGrantR<'a, consts::U1024>,
}

impl<'a> Message<'a> {
    fn view_with<'b: 'de, 'de, T, F, R>(&'b mut self, fun: F) -> Result<R, ()>
    where
        T: 'de + Deserialize<'de>,
        F: FnOnce(T) -> R,
        R: 'static,
    {
        match from_bytes(&self.fgr) {
            Ok(msg) => Ok(fun(msg)),
            Err(_e) => Err(()),
        }
    }
}

impl Rxer {
    fn rx_owned<T>(&mut self) -> Result<T, ()>
    where
        T: DeserializeOwned,
    {
        let rgr = self.cons.read().ok_or(())?;
        let result = from_bytes(&rgr);
        rgr.release();
        result.map_err(drop)
    }

    fn rx_view<'a, 'z: 'a>(&'z mut self) -> Result<Message<'a>, ()> {
        // TODO: I need to add a version of bbqueue grants that `release(rgr.len())`
        // the commit on drop, instead of `release(0)` on drop
        Ok(Message {
            fgr: self.cons.read().ok_or(())?,
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Eq, PartialEq)]
struct OwnedData {
    fuzz: [u8; 4],
}

#[derive(Debug, Serialize, Deserialize, Eq, PartialEq)]
struct BorrowedData<'a> {
    fuzz: &'a [u8],
}

fn main() {
    let (prod, cons) = BB.try_split_framed().unwrap();
    let mut prod = Txer { prod };
    let mut cons = Rxer { cons };

    // Owned data
    let tx = OwnedData {
        fuzz: [10, 20, 30, 40],
    };

    prod.send(&tx).unwrap();
    let rx = cons.rx_owned::<OwnedData>().unwrap();
    assert_eq!(tx, rx);

    let to_borrow = &[5, 10, 15, 20, 25];

    let tx = BorrowedData { fuzz: to_borrow };
    prod.send(&tx).unwrap();
    let rx = cons.rx_view().and_then(|mut pkt| {
        pkt.view_with(|msg: BorrowedData| {
            assert_eq!(tx, msg);
            true
        })
    });

    assert!(rx.unwrap());
}
