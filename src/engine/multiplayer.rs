use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IgeHandlerSnapshot {
  pub iid: u64,
  pub players: HashMap<u64, PlayerData>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GarbageRecord {
  pub iid: u64,
  pub amount: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerData {
  pub incoming: u64,
  pub outgoing: Vec<GarbageRecord>,
}

#[derive(Debug, Clone)]
pub struct IgeHandler {
  pub opponents: Vec<u64>,
  players: HashMap<u64, PlayerData>,
  iid_counter: u64,
}

impl IgeHandler {
  pub fn new(opponents: Vec<u64>) -> Self {
    let players = opponents
      .iter()
      .map(|&id| {
        (
          id,
          PlayerData {
            incoming: 0,
            outgoing: Vec::new(),
          },
        )
      })
      .collect();
    IgeHandler {
      opponents,
      players,
      iid_counter: 0,
    }
  }

  pub fn send(&mut self, target: u64, amount: u32) -> u64 {
    if amount == 0 {
      return 0;
    }

    self.iid_counter += 1;
    let iid = self.iid_counter;

    self
      .players
      .entry(target)
      .or_insert_with(|| PlayerData {
        incoming: 0,
        outgoing: Vec::new(),
      })
      .outgoing
      .push(GarbageRecord { iid, amount });

    iid
  }

  pub fn receive(&mut self, gameid: u64, ackiid: u64, iid: u64, amount: u32) -> u32 {
    let player = self.players.entry(gameid).or_insert_with(|| PlayerData {
      incoming: 0,
      outgoing: Vec::new(),
    });

    let incoming_iid = iid.max(player.incoming);
    let mut new_outgoing = Vec::new();
    let mut running_amount: u32 = amount;

    for mut item in player.outgoing.clone() {
      if item.iid <= ackiid {
        continue;
      }

      let cancel = item.amount.min(running_amount);
      item.amount -= cancel;
      running_amount -= cancel;

      if item.amount > 0 {
        new_outgoing.push(item);
      }
    }

    player.incoming = incoming_iid;
    player.outgoing = new_outgoing;

    running_amount
  }

  pub fn reset(&mut self) {
    for player in self.players.values_mut() {
      player.incoming = 0;
      player.outgoing.clear();
    }
    self.iid_counter = 0;
  }

  pub fn snapshot(&self) -> IgeHandlerSnapshot {
    IgeHandlerSnapshot {
      iid: self.iid_counter,
      players: self.players.clone(),
    }
  }

  pub fn from_snapshot(&mut self, snapshot: &IgeHandlerSnapshot) {
    self.players = snapshot.players.clone();
    self.iid_counter = snapshot.iid;
  }
}
