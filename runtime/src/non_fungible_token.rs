use crate::linked_item::{LinkedItem, LinkedList};
use parity_codec::{Decode, Encode};
use rstd::result;
use runtime_io::blake2_256;
use runtime_primitives::traits::{As, Bounded, Member, One, SimpleArithmetic};
use support::{
    decl_module, decl_storage, dispatch::Result, ensure, Parameter, StorageMap, StorageValue,
};
use system::ensure_signed;

pub trait Trait: system::Trait {
    type NFTIndex: Parameter + Member + Default + SimpleArithmetic + Bounded + Copy;
}

type NFTokenId = [u8; 32];

#[derive(Encode, Decode, Clone)]
pub struct NFToken {
    pub token_id: NFTokenId,
    pub lifetime: u64,
}

type NFTLinkedItem<T> = LinkedItem<<T as Trait>::NFTIndex>;
type OwnedNFTsList<T> =
    LinkedList<OwnedNFTs<T>, <T as system::Trait>::AccountId, <T as Trait>::NFTIndex>;

//impl PRC721Metadata {
//    pub fn tokenURI(owner: T::AccountId) -> String {
//        return Self.tokenUrl;
//    }
//    pub fn setTokenURI(tokenId: Uint256, uri: String) {}
//}

decl_storage! {
	trait Store for Module<T: Trait> as PRC721 {
		// Mapping from token ID to owner
        //mapping (Uint256 => address) private _tokenOwner;
		TokenOwner get(owned_token): map (Uint256) => Option<T::AccountId>;

		// Mapping from token ID to approved address
        //mapping (Uint256 => address) private _tokenApprovals;
		TokenApprovals get(approval_token): map(Uint256) => Option<T::AccountId>;

		// Mapping from owner to number of owned token
		//mapping (address => Counters.Counter) private _ownedTokensCount;
		OwnedTokensCount get(owned_token_count): map(T::AccountId) => Uint256;

		// Mapping from owner to operator approvals
		//mapping (address => mapping (address => bool)) private _operatorApprovals;
		OperatorApprovals get(approval_operator): map(T::AccountId) => (T::AccountId, bool);
	}
}

decl_module! {
    pub struct Module<T: Trait> for enum Call where origin: T::Origin {

        fn deposit_event<T>() = default;

        // Start ERC721 : Public Functions //
        fn approve(origin, to: T::AccountId, token_id: T::Hash) -> Result {
            let sender = ensure_signed(origin)?;
            let owner = match Self::owner_of(token_id) {
                Some(c) => c,
                None => return Err("No owner for this token"),
            };

            ensure!(to != owner, "Owner is implicitly approved");
            ensure!(sender == owner || Self::is_approved_for_all((owner.clone(), sender.clone())), "You are not allowed to approve for this token");

            <TokenApprovals<T>>::insert(&token_id, &to);

            Self::deposit_event(RawEvent::Approval(owner, to, token_id));

            Ok(())
        }

        fn set_approval_for_all(origin, to: T::AccountId, approved: bool) -> Result {
            let sender = ensure_signed(origin)?;
            ensure!(to != sender, "You are already implicity approved for your own actions");
            <OperatorApprovals<T>>::insert((sender.clone(), to.clone()), approved);

            Self::deposit_event(RawEvent::ApprovalForAll(sender, to, approved));

            Ok(())
        }

        // transfer_from will transfer to addresses even without a balance
        fn transfer_from(origin, from: T::AccountId, to: T::AccountId, token_id: T::Hash) -> Result {
            let sender = ensure_signed(origin)?;
            ensure!(Self::_is_approved_or_owner(sender, token_id), "You do not own this token");

            Self::_transfer_from(from, to, token_id)?;

            Ok(())
        }

        // safe_transfer_from checks that the recieving address has enough balance to satisfy the ExistentialDeposit
        // This is not quite what it does on Ethereum, but in the same spirit...
        fn safe_transfer_from(origin, from: T::AccountId, to: T::AccountId, token_id: T::Hash) -> Result {
            let to_balance = <balances::Module<T>>::free_balance(&to);
            ensure!(!to_balance.is_zero(), "'to' account does not satisfy the `ExistentialDeposit` requirement");

            Self::transfer_from(origin, from, to, token_id)?;

            Ok(())
        }
        // End ERC721 : Public Functions //

        // Not part of ERC721, but allows you to play with the runtime
        fn create_token(origin) -> Result {
            let sender = ensure_signed(origin)?;
            let nonce = <Nonce<T>>::get();
            let random_hash = (<system::Module<T>>::random_seed(), &sender, nonce).using_encoded(<T as system::Trait>::Hashing::hash);
            
            Self::_mint(sender, random_hash)?;
            <Nonce<T>>::mutate(|n| *n += 1);

            Ok(())
        }
    }
}

impl<T: Trait> Module<T> {
    // Start ERC721 : Internal Functions //
    fn _exists(token_id: T::Hash) -> bool {
        return <TokenOwner<T>>::exists(token_id);
    }

    fn _is_approved_or_owner(spender: T::AccountId, token_id: T::Hash) -> bool {
        let owner = Self::owner_of(token_id);
        let approved_user = Self::get_approved(token_id);

        let approved_as_owner = match owner {
            Some(ref o) => o == &spender,
            None => false,
        };

        let approved_as_delegate = match owner {
            Some(d) => Self::is_approved_for_all((d, spender.clone())),
            None => false,
        };

        let approved_as_user = match approved_user {
            Some(u) => u == spender,
            None => false,
        };

        return approved_as_owner || approved_as_user || approved_as_delegate
    }

    fn _mint(to: T::AccountId, token_id: T::Hash) -> Result {
        ensure!(!Self::_exists(token_id), "Token already exists");

        let balance_of = Self::balance_of(&to);

        let new_balance_of = match balance_of.checked_add(1) {
            Some(c) => c,
            None => return Err("Overflow adding a new token to account balance"),
        };

        // Writing to storage begins here
        Self::_add_token_to_all_tokens_enumeration(token_id)?;
        Self::_add_token_to_owner_enumeration(to.clone(), token_id)?;

        <TokenOwner<T>>::insert(token_id, &to);
        <OwnedTokensCount<T>>::insert(&to, new_balance_of);

        Self::deposit_event(RawEvent::Transfer(None, Some(to), token_id));

        Ok(())
    }

    fn _burn(token_id: T::Hash) -> Result {
        let owner = match Self::owner_of(token_id) {
            Some(c) => c,
            None => return Err("No owner for this token"),
        };

        let balance_of = Self::balance_of(&owner);

        let new_balance_of = match balance_of.checked_sub(1) {
            Some(c) => c,
            None => return Err("Underflow subtracting a token to account balance"),
        };

        // Writing to storage begins here
        Self::_remove_token_from_all_tokens_enumeration(token_id)?;
        Self::_remove_token_from_owner_enumeration(owner.clone(), token_id)?;
        <OwnedTokensIndex<T>>::remove(token_id);

        Self::_clear_approval(token_id)?;

        <OwnedTokensCount<T>>::insert(&owner, new_balance_of);
        <TokenOwner<T>>::remove(token_id);

        Self::deposit_event(RawEvent::Transfer(Some(owner), None, token_id));

        Ok(())
    }

    fn _transfer_from(from: T::AccountId, to: T::AccountId, token_id: T::Hash) -> Result {
        let owner = match Self::owner_of(token_id) {
            Some(c) => c,
            None => return Err("No owner for this token"),
        };

        ensure!(owner == from, "'from' account does not own this token");

        let balance_of_from = Self::balance_of(&from);
        let balance_of_to = Self::balance_of(&to);

        let new_balance_of_from = match balance_of_from.checked_sub(1) {
            Some (c) => c,
            None => return Err("Transfer causes underflow of 'from' token balance"),
        };

        let new_balance_of_to = match balance_of_to.checked_add(1) {
            Some(c) => c,
            None => return Err("Transfer causes overflow of 'to' token balance"),
        };

        // Writing to storage begins here
        Self::_remove_token_from_owner_enumeration(from.clone(), token_id)?;
        Self::_add_token_to_owner_enumeration(to.clone(), token_id)?;
        
        Self::_clear_approval(token_id)?;
        <OwnedTokensCount<T>>::insert(&from, new_balance_of_from);
        <OwnedTokensCount<T>>::insert(&to, new_balance_of_to);
        <TokenOwner<T>>::insert(&token_id, &to);

        Self::deposit_event(RawEvent::Transfer(Some(from), Some(to), token_id));
        
        Ok(())
    }

    fn _clear_approval(token_id: T::Hash) -> Result{
        <TokenApprovals<T>>::remove(token_id);

        Ok(())
    }
    // End ERC721 : Internal Functions //

    // Start ERC721 : Enumerable : Internal Functions //
    fn _add_token_to_owner_enumeration(to: T::AccountId, token_id: T::Hash) -> Result {
        let new_token_index = Self::balance_of(&to);

        <OwnedTokensIndex<T>>::insert(token_id, new_token_index);
        <OwnedTokens<T>>::insert((to, new_token_index), token_id);

        Ok(())
    }

    fn _add_token_to_all_tokens_enumeration(token_id: T::Hash) -> Result {
        let total_supply = Self::total_supply();

        // Should never fail since overflow on user balance is checked before this
        let new_total_supply = match total_supply.checked_add(1) {
            Some (c) => c,
            None => return Err("Overflow when adding new token to total supply"),
        };

        let new_token_index = total_supply;

        <AllTokensIndex<T>>::insert(token_id, new_token_index);
        <AllTokens<T>>::insert(new_token_index, token_id);
        <TotalSupply<T>>::put(new_total_supply);

        Ok(())
    }

    fn _remove_token_from_owner_enumeration(from: T::AccountId, token_id: T::Hash) -> Result {
        let balance_of_from = Self::balance_of(&from);

        // Should never fail because same check happens before this call is made
        let last_token_index = match balance_of_from.checked_sub(1) {
            Some (c) => c,
            None => return Err("Transfer causes underflow of 'from' token balance"),
        };
        
        let token_index = <OwnedTokensIndex<T>>::get(token_id);

        if token_index != last_token_index {
            let last_token_id = <OwnedTokens<T>>::get((from.clone(), last_token_index));
            <OwnedTokens<T>>::insert((from.clone(), token_index), last_token_id);
            <OwnedTokensIndex<T>>::insert(last_token_id, token_index);
        }

        <OwnedTokens<T>>::remove((from, last_token_index));
        // OpenZeppelin does not do this... should I?
        <OwnedTokensIndex<T>>::remove(token_id);

        Ok(())
    }

    fn _remove_token_from_all_tokens_enumeration(token_id: T::Hash) -> Result {
        let total_supply = Self::total_supply();

        // Should never fail because balance of underflow is checked before this
        let new_total_supply = match total_supply.checked_sub(1) {
            Some(c) => c,
            None => return Err("Underflow removing token from total supply"),
        };

        let last_token_index = new_total_supply;

        let token_index = <AllTokensIndex<T>>::get(token_id);

        let last_token_id = <AllTokens<T>>::get(last_token_index);

        <AllTokens<T>>::insert(token_index, last_token_id);
        <AllTokensIndex<T>>::insert(last_token_id, token_index);

        <AllTokens<T>>::remove(last_token_index);
        <AllTokensIndex<T>>::remove(token_id);

        <TotalSupply<T>>::put(new_total_supply);

        Ok(())
    }
    // End ERC721 : Enumerable : Internal Functions //
}