use std::collections::{BTreeMap, HashMap, VecDeque};

use tokio::sync::broadcast;
use uuid::Uuid;

use crate::models::{Order, OrderSide, OrderStatus, OrderType, PriceLevel, TradeEvent};

pub struct MarketState {
    pub books: HashMap<String, OrderBook>,
    pub orders: HashMap<Uuid, Order>,
    pub trade_tx: broadcast::Sender<TradeEvent>,
    pub last_prices: HashMap<String, u64>,
}

impl MarketState {
    pub fn new(capacity: usize) -> Self {
        let (trade_tx, _) = broadcast::channel(capacity);
        Self {
            books: HashMap::new(),
            orders: HashMap::new(),
            trade_tx,
            last_prices: HashMap::new(),
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<TradeEvent> {
        self.trade_tx.subscribe()
    }

    pub fn place_order(&mut self, order: Order) -> Vec<TradeEvent> {
        let good_name = order.good_name.clone();
        let _book = self.books.entry(good_name.clone()).or_insert_with(OrderBook::new);
        let order_id = order.id;
        self.orders.insert(order_id, order);

        let trades = self.match_order(order_id);

        // After matching, if limit order has remaining quantity, rest in book
        if let Some(order) = self.orders.get(&order_id) {
            if order.remaining() > 0 {
                match order.order_type {
                    OrderType::Limit => {
                        let book = self.books.entry(good_name).or_insert_with(OrderBook::new);
                        let price = order.price.expect("invariant: limit order always has price");
                        match order.side {
                            OrderSide::Buy => {
                                book.bids.entry(price).or_default().push_back(order_id);
                            }
                            OrderSide::Sell => {
                                book.asks.entry(price).or_default().push_back(order_id);
                            }
                        }
                    }
                    OrderType::Market => {
                        // Market orders that can't fully fill: cancel remainder
                        if let Some(order) = self.orders.get_mut(&order_id) {
                            if order.filled_quantity > 0 {
                                order.status = OrderStatus::PartiallyFilled;
                            } else {
                                order.status = OrderStatus::Cancelled;
                            }
                        }
                    }
                }
            }
        }

        trades
    }

    fn match_order(&mut self, order_id: Uuid) -> Vec<TradeEvent> {
        let mut trades = Vec::new();

        let order = match self.orders.get(&order_id) {
            Some(o) => o.clone(),
            None => return trades,
        };

        match order.side {
            OrderSide::Buy => {
                trades = self.match_buy_order(order_id);
            }
            OrderSide::Sell => {
                trades = self.match_sell_order(order_id);
            }
        }

        trades
    }

    fn match_buy_order(&mut self, buy_id: Uuid) -> Vec<TradeEvent> {
        let mut trades = Vec::new();

        loop {
            let buy_order = match self.orders.get(&buy_id) {
                Some(o) if o.remaining() > 0 => o.clone(),
                _ => break,
            };

            let good_name = buy_order.good_name.clone();
            let book = match self.books.get_mut(&good_name) {
                Some(b) => b,
                None => break,
            };

            // Find best ask (lowest price)
            let best_ask_price = match book.asks.keys().next().copied() {
                Some(p) => p,
                None => break,
            };

            // Check price compatibility
            match buy_order.order_type {
                OrderType::Limit => {
                    if best_ask_price > buy_order.price.expect("invariant: limit order always has price") {
                        break;
                    }
                }
                OrderType::Market => {} // any price
            }

            // Get the first order at this price level
            let ask_queue = book.asks.get_mut(&best_ask_price).expect("invariant: best ask price was just found");
            let ask_id = match ask_queue.front().copied() {
                Some(id) => id,
                None => {
                    book.asks.remove(&best_ask_price);
                    continue;
                }
            };

            let ask_order = match self.orders.get(&ask_id) {
                Some(o) => o.clone(),
                None => {
                    ask_queue.pop_front();
                    if ask_queue.is_empty() {
                        book.asks.remove(&best_ask_price);
                    }
                    continue;
                }
            };

            let fill_qty = buy_order.remaining().min(ask_order.remaining());
            let trade_price = best_ask_price; // Trade at ask price for buys

            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock before UNIX epoch")
                .as_millis() as u64;

            let trade = TradeEvent {
                id: Uuid::new_v4(),
                good_name: good_name.clone(),
                price: trade_price,
                quantity: fill_qty,
                buyer_id: buy_order.player_id.clone(),
                seller_id: ask_order.player_id.clone(),
                buyer_station: buy_order.station_planet_id.clone(),
                seller_station: ask_order.station_planet_id.clone(),
                timestamp: now,
            };

            // Update both orders
            if let Some(buy) = self.orders.get_mut(&buy_id) {
                buy.filled_quantity += fill_qty;
                if buy.remaining() == 0 {
                    buy.status = OrderStatus::Filled;
                } else {
                    buy.status = OrderStatus::PartiallyFilled;
                }
            }
            if let Some(ask) = self.orders.get_mut(&ask_id) {
                ask.filled_quantity += fill_qty;
                if ask.remaining() == 0 {
                    ask.status = OrderStatus::Filled;
                } else {
                    ask.status = OrderStatus::PartiallyFilled;
                }
            }

            // Remove filled ask from book
            let book = self.books.get_mut(&good_name).expect("invariant: book was just accessed");
            if let Some(ask_order) = self.orders.get(&ask_id) {
                if ask_order.remaining() == 0 {
                    if let Some(queue) = book.asks.get_mut(&best_ask_price) {
                        queue.pop_front();
                        if queue.is_empty() {
                            book.asks.remove(&best_ask_price);
                        }
                    }
                }
            }

            self.last_prices.insert(good_name.clone(), trade_price);

            // Broadcast trade (ignore error if no subscribers)
            let _ = self.trade_tx.send(trade.clone());

            trades.push(trade);
        }

        trades
    }

    fn match_sell_order(&mut self, sell_id: Uuid) -> Vec<TradeEvent> {
        let mut trades = Vec::new();

        loop {
            let sell_order = match self.orders.get(&sell_id) {
                Some(o) if o.remaining() > 0 => o.clone(),
                _ => break,
            };

            let good_name = sell_order.good_name.clone();
            let book = match self.books.get_mut(&good_name) {
                Some(b) => b,
                None => break,
            };

            // Find best bid (highest price)
            let best_bid_price = match book.bids.keys().next_back().copied() {
                Some(p) => p,
                None => break,
            };

            // Check price compatibility
            match sell_order.order_type {
                OrderType::Limit => {
                    if best_bid_price < sell_order.price.expect("invariant: limit order always has price") {
                        break;
                    }
                }
                OrderType::Market => {} // any price
            }

            // Get the first order at this price level
            let bid_queue = book.bids.get_mut(&best_bid_price).expect("invariant: best bid price was just found");
            let bid_id = match bid_queue.front().copied() {
                Some(id) => id,
                None => {
                    book.bids.remove(&best_bid_price);
                    continue;
                }
            };

            let bid_order = match self.orders.get(&bid_id) {
                Some(o) => o.clone(),
                None => {
                    bid_queue.pop_front();
                    if bid_queue.is_empty() {
                        book.bids.remove(&best_bid_price);
                    }
                    continue;
                }
            };

            let fill_qty = sell_order.remaining().min(bid_order.remaining());
            let trade_price = best_bid_price; // Trade at bid price for sells

            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock before UNIX epoch")
                .as_millis() as u64;

            let trade = TradeEvent {
                id: Uuid::new_v4(),
                good_name: good_name.clone(),
                price: trade_price,
                quantity: fill_qty,
                buyer_id: bid_order.player_id.clone(),
                seller_id: sell_order.player_id.clone(),
                buyer_station: bid_order.station_planet_id.clone(),
                seller_station: sell_order.station_planet_id.clone(),
                timestamp: now,
            };

            // Update both orders
            if let Some(sell) = self.orders.get_mut(&sell_id) {
                sell.filled_quantity += fill_qty;
                if sell.remaining() == 0 {
                    sell.status = OrderStatus::Filled;
                } else {
                    sell.status = OrderStatus::PartiallyFilled;
                }
            }
            if let Some(bid) = self.orders.get_mut(&bid_id) {
                bid.filled_quantity += fill_qty;
                if bid.remaining() == 0 {
                    bid.status = OrderStatus::Filled;
                } else {
                    bid.status = OrderStatus::PartiallyFilled;
                }
            }

            // Remove filled bid from book
            let book = self.books.get_mut(&good_name).expect("invariant: book was just accessed");
            if let Some(bid_order) = self.orders.get(&bid_id) {
                if bid_order.remaining() == 0 {
                    if let Some(queue) = book.bids.get_mut(&best_bid_price) {
                        queue.pop_front();
                        if queue.is_empty() {
                            book.bids.remove(&best_bid_price);
                        }
                    }
                }
            }

            self.last_prices.insert(good_name.clone(), trade_price);

            let _ = self.trade_tx.send(trade.clone());

            trades.push(trade);
        }

        trades
    }

    pub fn get_order_book_summary(&self, good_name: &str) -> OrderBookSummary {
        let book = self.books.get(good_name);
        let mut bids = Vec::new();
        let mut asks = Vec::new();

        if let Some(book) = book {
            // Bids: highest first
            for (&price, queue) in book.bids.iter().rev() {
                let mut total_quantity = 0u64;
                let mut order_count = 0u32;
                for &order_id in queue {
                    if let Some(order) = self.orders.get(&order_id) {
                        if order.remaining() > 0 {
                            total_quantity += order.remaining();
                            order_count += 1;
                        }
                    }
                }
                if order_count > 0 {
                    bids.push(PriceLevel {
                        price,
                        total_quantity,
                        order_count,
                    });
                }
            }

            // Asks: lowest first
            for (&price, queue) in book.asks.iter() {
                let mut total_quantity = 0u64;
                let mut order_count = 0u32;
                for &order_id in queue {
                    if let Some(order) = self.orders.get(&order_id) {
                        if order.remaining() > 0 {
                            total_quantity += order.remaining();
                            order_count += 1;
                        }
                    }
                }
                if order_count > 0 {
                    asks.push(PriceLevel {
                        price,
                        total_quantity,
                        order_count,
                    });
                }
            }
        }

        OrderBookSummary {
            good_name: good_name.to_string(),
            bids,
            asks,
            last_trade_price: self.last_prices.get(good_name).copied(),
        }
    }

    pub fn cancel_order(&mut self, order_id: Uuid) -> Option<Order> {
        let order = self.orders.get_mut(&order_id)?;

        match order.status {
            OrderStatus::Open | OrderStatus::PartiallyFilled => {
                order.status = OrderStatus::Cancelled;
                let order = order.clone();

                // Remove from book only for limit orders (market orders are never in the book)
                if let Some(price) = order.price {
                    if let Some(book) = self.books.get_mut(&order.good_name) {
                        match order.side {
                            OrderSide::Buy => {
                                if let Some(queue) = book.bids.get_mut(&price) {
                                    queue.retain(|&id| id != order_id);
                                    if queue.is_empty() {
                                        book.bids.remove(&price);
                                    }
                                }
                            }
                            OrderSide::Sell => {
                                if let Some(queue) = book.asks.get_mut(&price) {
                                    queue.retain(|&id| id != order_id);
                                    if queue.is_empty() {
                                        book.asks.remove(&price);
                                    }
                                }
                            }
                        }
                    }
                }

                Some(order)
            }
            _ => None,
        }
    }
}

pub struct OrderBook {
    pub bids: BTreeMap<u64, VecDeque<Uuid>>,
    pub asks: BTreeMap<u64, VecDeque<Uuid>>,
}

impl OrderBook {
    pub fn new() -> Self {
        Self {
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
        }
    }
}

use crate::models::OrderBookSummary;
