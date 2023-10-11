
const INTERNAL_ERR: &'static str = "`ethabi_derive` internal error";

// pub mod bonded {
//     use sp_std::borrow::ToOwned;
//     use ethabi;
//     use super::INTERNAL_ERR;
//     pub fn event() -> ethabi::Event {
//         ethabi::Event {
//             name: "Bonded".into(),
//             inputs: <[_]>::into_vec(
//                 #[rustc_box]
//                     Box::new([
//                     ethabi::EventParam {
//                         name: "".to_owned(),
//                         kind: ethabi::ParamType::String,
//                         indexed: false,
//                     },
//                     ethabi::EventParam {
//                         name: "".to_owned(),
//                         kind: ethabi::ParamType::Uint(256usize),
//                         indexed: true,
//                     },
//                     ethabi::EventParam {
//                         name: "".to_owned(),
//                         kind: ethabi::ParamType::Uint(8usize),
//                         indexed: true,
//                     },
//                 ]),
//             ),
//             anonymous: false,
//         }
//     }
//     pub fn filter<
//         T0: Into<ethabi::Topic<ethabi::Uint>>,
//         T1: Into<ethabi::Topic<ethabi::Uint>>,
//     >(topic1: T0, topic2: T1) -> ethabi::TopicFilter {
//         let raw = ethabi::RawTopicFilter {
//             topic0: topic1.into().map(|i| ethabi::Token::Uint(i)),
//             topic1: topic2.into().map(|i| ethabi::Token::Uint(i)),
//             ..Default::default()
//         };
//         let e = event();
//         e.filter(raw).expect(INTERNAL_ERR)
//     }
//     pub fn wildcard_filter() -> ethabi::TopicFilter {
//         filter(ethabi::Topic::Any, ethabi::Topic::Any)
//     }
//     pub fn parse_log(
//         log: ethabi::RawLog,
//     ) -> ethabi::Result<super::super::logs::Bonded> {
//         let e = event();
//         let mut log = e.parse_log(log)?.params.into_iter();
//         let result = super::super::logs::Bonded {
//             param0: log
//                 .next()
//                 .expect(INTERNAL_ERR)
//                 .value
//                 .into_string()
//                 .expect(INTERNAL_ERR),
//             topic1: log
//                 .next()
//                 .expect(INTERNAL_ERR)
//                 .value
//                 .into_uint()
//                 .expect(INTERNAL_ERR),
//             topic2: log
//                 .next()
//                 .expect(INTERNAL_ERR)
//                 .value
//                 .into_uint()
//                 .expect(INTERNAL_ERR),
//         };
//         Ok(result)
//     }
// }
// pub mod extra_bonded {
//     use sp_std::borrow::ToOwned;
//     use ethabi;
//     use super::INTERNAL_ERR;
//     pub fn event() -> ethabi::Event {
//         ethabi::Event {
//             name: "ExtraBonded".into(),
//             inputs: <[_]>::into_vec(
//                 #[rustc_box]
//                     Box::new([
//                     ethabi::EventParam {
//                         name: "".to_owned(),
//                         kind: ethabi::ParamType::Address,
//                         indexed: false,
//                     },
//                     ethabi::EventParam {
//                         name: "".to_owned(),
//                         kind: ethabi::ParamType::Uint(256usize),
//                         indexed: true,
//                     },
//                 ]),
//             ),
//             anonymous: false,
//         }
//     }
//     pub fn filter<T0: Into<ethabi::Topic<ethabi::Uint>>>(
//         topic1: T0,
//     ) -> ethabi::TopicFilter {
//         let raw = ethabi::RawTopicFilter {
//             topic0: topic1.into().map(|i| ethabi::Token::Uint(i)),
//             ..Default::default()
//         };
//         let e = event();
//         e.filter(raw).expect(INTERNAL_ERR)
//     }
//     pub fn wildcard_filter() -> ethabi::TopicFilter {
//         filter(ethabi::Topic::Any)
//     }
//     pub fn parse_log(
//         log: ethabi::RawLog,
//     ) -> ethabi::Result<super::super::logs::ExtraBonded> {
//         let e = event();
//         let mut log = e.parse_log(log)?.params.into_iter();
//         let result = super::super::logs::ExtraBonded {
//             param0: log
//                 .next()
//                 .expect(INTERNAL_ERR)
//                 .value
//                 .into_address()
//                 .expect(INTERNAL_ERR),
//             topic1: log
//                 .next()
//                 .expect(INTERNAL_ERR)
//                 .value
//                 .into_uint()
//                 .expect(INTERNAL_ERR),
//         };
//         Ok(result)
//     }
// }
pub mod nominated {
    use sp_std::borrow::ToOwned;
    use ethabi;
    use crate::Box;
    use crate::events::Nominated;
    use super::INTERNAL_ERR;
    pub fn event() -> ethabi::Event {
        ethabi::Event {
            name: "Nominated".into(),
            inputs: <[_]>::into_vec(
                    Box::new([
                    ethabi::EventParam {
                        name: "".to_owned(),
                        kind: ethabi::ParamType::Address,
                        indexed: false,
                    },
                    ethabi::EventParam {
                        name: "".to_owned(),
                        kind: ethabi::ParamType::Array(
                            Box::new(ethabi::ParamType::String),
                        ),
                        indexed: false,
                    },
                ]),
            ),
            anonymous: false,
        }
    }
    pub fn filter() -> ethabi::TopicFilter {
        let raw = ethabi::RawTopicFilter {
            ..Default::default()
        };
        let e = event();
        e.filter(raw).expect(INTERNAL_ERR)
    }
    pub fn wildcard_filter() -> ethabi::TopicFilter {
        filter()
    }
    pub fn parse_log(
        log: ethabi::RawLog,
    ) -> ethabi::Result<Nominated> {
        let e = event();
        let mut log = e.parse_log(log)?.params.into_iter();
        let result = Nominated {
            param0: log
                .next()
                .expect(INTERNAL_ERR)
                .value
                .into_address()
                .expect(INTERNAL_ERR),
            param1: log
                .next()
                .expect(INTERNAL_ERR)
                .value
                .into_array()
                .expect(INTERNAL_ERR)
                .into_iter()
                .map(|inner| inner.into_string().expect(INTERNAL_ERR))
                .collect(),
        };
        Ok(result)
    }
}
// pub mod setted_keys {
//     use sp_std::borrow::ToOwned;
//     use ethabi;
//     use super::INTERNAL_ERR;
//     pub fn event() -> ethabi::Event {
//         ethabi::Event {
//             name: "SettedKeys".into(),
//             inputs: <[_]>::into_vec(
//                 #[rustc_box]
//                     Box::new([
//                     ethabi::EventParam {
//                         name: "".to_owned(),
//                         kind: ethabi::ParamType::Address,
//                         indexed: false,
//                     },
//                     ethabi::EventParam {
//                         name: "".to_owned(),
//                         kind: ethabi::ParamType::Bytes,
//                         indexed: false,
//                     },
//                     ethabi::EventParam {
//                         name: "".to_owned(),
//                         kind: ethabi::ParamType::Bytes,
//                         indexed: false,
//                     },
//                 ]),
//             ),
//             anonymous: false,
//         }
//     }
//     pub fn filter() -> ethabi::TopicFilter {
//         let raw = ethabi::RawTopicFilter {
//             ..Default::default()
//         };
//         let e = event();
//         e.filter(raw).expect(INTERNAL_ERR)
//     }
//     pub fn wildcard_filter() -> ethabi::TopicFilter {
//         filter()
//     }
//     pub fn parse_log(
//         log: ethabi::RawLog,
//     ) -> ethabi::Result<super::super::logs::SettedKeys> {
//         let e = event();
//         let mut log = e.parse_log(log)?.params.into_iter();
//         let result = super::super::logs::SettedKeys {
//             param0: log
//                 .next()
//                 .expect(INTERNAL_ERR)
//                 .value
//                 .into_address()
//                 .expect(INTERNAL_ERR),
//             param1: log
//                 .next()
//                 .expect(INTERNAL_ERR)
//                 .value
//                 .into_bytes()
//                 .expect(INTERNAL_ERR),
//             param2: log
//                 .next()
//                 .expect(INTERNAL_ERR)
//                 .value
//                 .into_bytes()
//                 .expect(INTERNAL_ERR),
//         };
//         Ok(result)
//     }
// }
// pub mod un_bonded {
//     use sp_std::borrow::ToOwned;
//     use ethabi;
//     use super::INTERNAL_ERR;
//     pub fn event() -> ethabi::Event {
//         ethabi::Event {
//             name: "UnBonded".into(),
//             inputs: <[_]>::into_vec(
//                 #[rustc_box]
//                     Box::new([
//                     ethabi::EventParam {
//                         name: "".to_owned(),
//                         kind: ethabi::ParamType::Address,
//                         indexed: false,
//                     },
//                     ethabi::EventParam {
//                         name: "".to_owned(),
//                         kind: ethabi::ParamType::Uint(256usize),
//                         indexed: true,
//                     },
//                 ]),
//             ),
//             anonymous: false,
//         }
//     }
//     pub fn filter<T0: Into<ethabi::Topic<ethabi::Uint>>>(
//         topic1: T0,
//     ) -> ethabi::TopicFilter {
//         let raw = ethabi::RawTopicFilter {
//             topic0: topic1.into().map(|i| ethabi::Token::Uint(i)),
//             ..Default::default()
//         };
//         let e = event();
//         e.filter(raw).expect(INTERNAL_ERR)
//     }
//     pub fn wildcard_filter() -> ethabi::TopicFilter {
//         filter(ethabi::Topic::Any)
//     }
//     pub fn parse_log(
//         log: ethabi::RawLog,
//     ) -> ethabi::Result<super::super::logs::UnBonded> {
//         let e = event();
//         let mut log = e.parse_log(log)?.params.into_iter();
//         let result = super::super::logs::UnBonded {
//             param0: log
//                 .next()
//                 .expect(INTERNAL_ERR)
//                 .value
//                 .into_address()
//                 .expect(INTERNAL_ERR),
//             topic1: log
//                 .next()
//                 .expect(INTERNAL_ERR)
//                 .value
//                 .into_uint()
//                 .expect(INTERNAL_ERR),
//         };
//         Ok(result)
//     }
// }
// pub mod validated {
//     use sp_std::borrow::ToOwned;
//     use ethabi;
//     use super::INTERNAL_ERR;
//     pub fn event() -> ethabi::Event {
//         ethabi::Event {
//             name: "Validated".into(),
//             inputs: <[_]>::into_vec(
//                 #[rustc_box]
//                     Box::new([
//                     ethabi::EventParam {
//                         name: "".to_owned(),
//                         kind: ethabi::ParamType::Address,
//                         indexed: false,
//                     },
//                     ethabi::EventParam {
//                         name: "".to_owned(),
//                         kind: ethabi::ParamType::Uint(256usize),
//                         indexed: true,
//                     },
//                     ethabi::EventParam {
//                         name: "".to_owned(),
//                         kind: ethabi::ParamType::Bool,
//                         indexed: true,
//                     },
//                 ]),
//             ),
//             anonymous: false,
//         }
//     }
//     pub fn filter<
//         T0: Into<ethabi::Topic<ethabi::Uint>>,
//         T1: Into<ethabi::Topic<bool>>,
//     >(topic1: T0, topic2: T1) -> ethabi::TopicFilter {
//         let raw = ethabi::RawTopicFilter {
//             topic0: topic1.into().map(|i| ethabi::Token::Uint(i)),
//             topic1: topic2.into().map(|i| ethabi::Token::Bool(i)),
//             ..Default::default()
//         };
//         let e = event();
//         e.filter(raw).expect(INTERNAL_ERR)
//     }
//     pub fn wildcard_filter() -> ethabi::TopicFilter {
//         filter(ethabi::Topic::Any, ethabi::Topic::Any)
//     }
//     pub fn parse_log(
//         log: ethabi::RawLog,
//     ) -> ethabi::Result<super::super::logs::Validated> {
//         let e = event();
//         let mut log = e.parse_log(log)?.params.into_iter();
//         let result = super::super::logs::Validated {
//             param0: log
//                 .next()
//                 .expect(INTERNAL_ERR)
//                 .value
//                 .into_address()
//                 .expect(INTERNAL_ERR),
//             topic1: log
//                 .next()
//                 .expect(INTERNAL_ERR)
//                 .value
//                 .into_uint()
//                 .expect(INTERNAL_ERR),
//             topic2: log
//                 .next()
//                 .expect(INTERNAL_ERR)
//                 .value
//                 .into_bool()
//                 .expect(INTERNAL_ERR),
//         };
//         Ok(result)
//     }
// }
// pub mod withdraw_un_bonded {
//     use sp_std::borrow::ToOwned;
//     use ethabi;
//     use super::INTERNAL_ERR;
//     pub fn event() -> ethabi::Event {
//         ethabi::Event {
//             name: "WithdrawUnBonded".into(),
//             inputs: <[_]>::into_vec(
//                 #[rustc_box]
//                     Box::new([
//                     ethabi::EventParam {
//                         name: "".to_owned(),
//                         kind: ethabi::ParamType::Address,
//                         indexed: false,
//                     },
//                     ethabi::EventParam {
//                         name: "".to_owned(),
//                         kind: ethabi::ParamType::Uint(256usize),
//                         indexed: true,
//                     },
//                 ]),
//             ),
//             anonymous: false,
//         }
//     }
//     pub fn filter<T0: Into<ethabi::Topic<ethabi::Uint>>>(
//         topic1: T0,
//     ) -> ethabi::TopicFilter {
//         let raw = ethabi::RawTopicFilter {
//             topic0: topic1.into().map(|i| ethabi::Token::Uint(i)),
//             ..Default::default()
//         };
//         let e = event();
//         e.filter(raw).expect(INTERNAL_ERR)
//     }
//     pub fn wildcard_filter() -> ethabi::TopicFilter {
//         filter(ethabi::Topic::Any)
//     }
//     pub fn parse_log(
//         log: ethabi::RawLog,
//     ) -> ethabi::Result<super::super::logs::WithdrawUnBonded> {
//         let e = event();
//         let mut log = e.parse_log(log)?.params.into_iter();
//         let result = super::super::logs::WithdrawUnBonded {
//             param0: log
//                 .next()
//                 .expect(INTERNAL_ERR)
//                 .value
//                 .into_address()
//                 .expect(INTERNAL_ERR),
//             topic1: log
//                 .next()
//                 .expect(INTERNAL_ERR)
//                 .value
//                 .into_uint()
//                 .expect(INTERNAL_ERR),
//         };
//         Ok(result)
//     }
// }
//
// /// Contract's logs.
// use ethabi;
// pub struct Bonded {
//     pub param0: String,
//     pub topic1: ethabi::Uint,
//     pub topic2: ethabi::Uint,
// }
//
// #[automatically_derived]
// impl ::core::clone::Clone for Bonded {
//     #[inline]
//     fn clone(&self) -> Bonded {
//         Bonded {
//             param0: ::core::clone::Clone::clone(&self.param0),
//             topic1: ::core::clone::Clone::clone(&self.topic1),
//             topic2: ::core::clone::Clone::clone(&self.topic2),
//         }
//     }
// }
//
// pub struct ExtraBonded {
//     pub param0: ethabi::Address,
//     pub topic1: ethabi::Uint,
// }
//
// #[automatically_derived]
// impl ::core::clone::Clone for ExtraBonded {
//     #[inline]
//     fn clone(&self) -> ExtraBonded {
//         ExtraBonded {
//             param0: ::core::clone::Clone::clone(&self.param0),
//             topic1: ::core::clone::Clone::clone(&self.topic1),
//         }
//     }
// }
//
use crate::Vec;
use scale_info::prelude::string::String;
pub struct Nominated {
    pub param0: ethabi::Address,
    pub param1: Vec<String>,
}
//
// #[automatically_derived]
// impl ::core::clone::Clone for Nominated {
//     #[inline]
//     fn clone(&self) -> Nominated {
//         Nominated {
//             param0: ::core::clone::Clone::clone(&self.param0),
//             param1: ::core::clone::Clone::clone(&self.param1),
//         }
//     }
// }
//
// pub struct SettedKeys {
//     pub param0: ethabi::Address,
//     pub param1: ethabi::Bytes,
//     pub param2: ethabi::Bytes,
// }
//
// #[automatically_derived]
// impl ::core::clone::Clone for SettedKeys {
//     #[inline]
//     fn clone(&self) -> SettedKeys {
//         SettedKeys {
//             param0: ::core::clone::Clone::clone(&self.param0),
//             param1: ::core::clone::Clone::clone(&self.param1),
//             param2: ::core::clone::Clone::clone(&self.param2),
//         }
//     }
// }
//
// pub struct UnBonded {
//     pub param0: ethabi::Address,
//     pub topic1: ethabi::Uint,
// }
//
// #[automatically_derived]
// impl ::core::clone::Clone for UnBonded {
//     #[inline]
//     fn clone(&self) -> UnBonded {
//         UnBonded {
//             param0: ::core::clone::Clone::clone(&self.param0),
//             topic1: ::core::clone::Clone::clone(&self.topic1),
//         }
//     }
// }
//
// pub struct Validated {
//     pub param0: ethabi::Address,
//     pub topic1: ethabi::Uint,
//     pub topic2: bool,
// }
//
// #[automatically_derived]
// impl ::core::clone::Clone for Validated {
//     #[inline]
//     fn clone(&self) -> Validated {
//         Validated {
//             param0: ::core::clone::Clone::clone(&self.param0),
//             topic1: ::core::clone::Clone::clone(&self.topic1),
//             topic2: ::core::clone::Clone::clone(&self.topic2),
//         }
//     }
// }
//
// pub struct WithdrawUnBonded {
//     pub param0: ethabi::Address,
//     pub topic1: ethabi::Uint,
// }
//
// #[automatically_derived]
// impl ::core::clone::Clone for WithdrawUnBonded {
//     #[inline]
//     fn clone(&self) -> WithdrawUnBonded {
//         WithdrawUnBonded {
//             param0: ::core::clone::Clone::clone(&self.param0),
//             topic1: ::core::clone::Clone::clone(&self.topic1),
//         }
//     }
// }
//
