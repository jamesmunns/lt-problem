use bbqueue::{
    BBBuffer, ConstBBBuffer, consts, framed::FrameProducer, framed::FrameConsumer,
};
use serde::{Serialize, Deserialize, de::DeserializeOwned};
use postcard::{to_slice, from_bytes};

static BB: BBBuffer<consts::U1024> = BBBuffer( ConstBBBuffer::new() );

struct Txer {
    prod: FrameProducer<'static, consts::U1024>,
}

impl Txer {
    fn send<T>(&mut self, msg: &T) -> Result<(), ()>
    where
        T: Serialize
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

impl Rxer {
    fn rx_owned<T>(&mut self) -> Result<T, ()>
    where
        T: DeserializeOwned
    {
        let rgr = self.cons.read().ok_or(())?;
        let result = from_bytes(&rgr);
        rgr.release();
        result.map_err(drop)
    }

    fn rx_with<'de, T, F, R>(&mut self, fun: F) -> Result<R, ()>
    where
        T: 'de + Deserialize<'de>,
        F: FnOnce(T) -> R,
        R: 'static
    {
        let rgr = self.cons.read().ok_or(())?;
        let result = match from_bytes(&rgr) {
            Ok(msg) => Ok(fun(msg)),
            Err(e) => Err(()),
        };

        rgr.release();
        result
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

    let tx = BorrowedData {
        fuzz: to_borrow,
    };
    prod.send(&tx).unwrap();
    let mut run = false;
    let rx = cons.rx_with(|recv: BorrowedData| {
        assert_eq!(recv, tx);
        run = true;
        true
    }).unwrap();

    assert!(rx);
    assert!(run);
}
