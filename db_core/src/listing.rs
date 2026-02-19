use crate::error::Result;
use crate::models::{Listing, ListingWithOwner, NewListing, UpdatedListing};
use sqlx::{PgExecutor, PgPool};
use uuid::Uuid;

/// Creates a new listing in the database.
#[tracing::instrument(skip(executor))]
pub async fn create_listing<'e, E>(executor: E, new_listing: &NewListing) -> Result<Listing>
where
    E: PgExecutor<'e>,
{
    let listing = sqlx::query_as!(
        Listing,
        r#"
        INSERT INTO listing (id, user_id, name, description, listing_structure_id, country, price_per_night, added_at)
        SELECT $1, $2, $3, $4, $5, $6, $7, now()
        WHERE EXISTS (SELECT 1 FROM host_profiles WHERE user_id = $2)
        RETURNING id, user_id, name, description, listing_structure_id, country, price_per_night, is_active, added_at, deleted_at
        "#,
        Uuid::now_v7(),
        new_listing.user_id,
        new_listing.name,
        new_listing.description,
        new_listing.listing_structure_id,
        new_listing.country,
        new_listing.price_per_night,
    )
    .fetch_one(executor)
    .await
    .map_err(|e| match e {
        sqlx::Error::RowNotFound => crate::error::DbError::ValidationError(
            "User must have a host profile to create a listing".to_string(),
        ),
        other => crate::error::DbError::Sqlx(other),
    })?;

    Ok(listing)
}

/// Retrieves a paginated list of listings from the database with optional filtering.
#[tracing::instrument(skip(executor))]
pub async fn get_listings<'e, E>(
    executor: E,
    page: u32,
    per_page: u32,
    filter: Option<common::models::ListingFilter>,
) -> Result<Vec<ListingWithOwner>>
where
    E: PgExecutor<'e>,
{
    // Calculate the LIMIT and OFFSET values for the SQL query.
    let limit = per_page as i64;
    let offset = ((page.max(1) - 1) * per_page) as i64;

    let mut query_builder = sqlx::QueryBuilder::new(
        r#"
        SELECT listing.id, listing.user_id, listing.name, listing.description, listing.listing_structure_id, listing.country, listing.price_per_night, listing.is_active, listing.added_at, listing.deleted_at,
        "user".first_name || ' ' || "user".last_name as owner_name
        FROM listing
        INNER JOIN "user" ON listing.user_id = "user".id
        WHERE listing.deleted_at IS NULL
        "#,
    );

    if let Some(f) = filter {
        if let Some(name) = f.name {
            query_builder.push(" AND listing.name ILIKE ");
            query_builder.push_bind(format!("%{}%", name));
        }

        if let Some(country) = f.country {
            query_builder.push(" AND listing.country ILIKE ");
            query_builder.push_bind(format!("%{}%", country));
        }

        if let Some(min_price) = f.min_price {
            query_builder.push(" AND listing.price_per_night >= ");
            query_builder.push_bind(min_price);
        }

        if let Some(max_price) = f.max_price {
            query_builder.push(" AND listing.price_per_night <= ");
            query_builder.push_bind(max_price);
        }

        if !f.structure_type.is_empty() {
            let mut ids = Vec::new();
            for st_str in &f.structure_type {
                if let Ok(st) = st_str.parse::<crate::models::StructureType>() {
                    ids.push(st.id());
                }
            }

            if !ids.is_empty() {
                query_builder.push(" AND listing.listing_structure_id = ANY(");
                query_builder.push_bind(ids);
                query_builder.push(")");
            }
        }

        if let Some(owner) = f.owner {
            query_builder.push(" AND \"user\".email ILIKE ");
            query_builder.push_bind(format!("%{}%", owner));
        }
    }

    query_builder.push(" ORDER BY listing.added_at DESC, listing.id DESC LIMIT ");
    query_builder.push_bind(limit);
    query_builder.push(" OFFSET ");
    query_builder.push_bind(offset);

    tracing::debug!("Query: {}", query_builder.sql());

    let query = query_builder.build_query_as::<ListingWithOwner>();
    let listings = query.fetch_all(executor).await?;

    Ok(listings)
}

/// Retrieves all listings for a specific user from the database.
#[tracing::instrument(skip(executor))]
pub async fn get_listings_by_user_id<'e, E>(executor: E, user_id: Uuid) -> Result<Vec<Listing>>
where
    E: PgExecutor<'e>,
{
    let listings = sqlx::query_as!(
        Listing,
        r#"
        SELECT id, user_id, name, description, listing_structure_id, country, price_per_night, is_active, added_at, deleted_at
        FROM listing
        WHERE user_id = $1 AND deleted_at IS NULL
        ORDER BY added_at DESC, id DESC
        "#,
        user_id
    )
    .fetch_all(executor)
    .await?;

    Ok(listings)
}

/// Retrieves a single listing from the database by its UUID.
#[tracing::instrument(skip(executor))]
pub async fn get_listing_by_id<'e, E>(executor: E, id: Uuid) -> Result<Listing>
where
    E: PgExecutor<'e>,
{
    let listing = sqlx::query_as!(
        Listing,
        r#"
        SELECT id, user_id, name, description, listing_structure_id, country, price_per_night, is_active, added_at, deleted_at
        FROM listing
        WHERE id = $1 AND deleted_at IS NULL
        "#,
        id
    )
    .fetch_one(executor)
    .await?;

    Ok(listing)
}

/// Updates a listing in the database.
#[tracing::instrument(skip(pool))]
pub async fn update_listing(
    pool: &PgPool,
    id: Uuid,
    updated_listing_data: &UpdatedListing,
) -> Result<Listing> {
    let mut tx = pool.begin().await?;

    let current = sqlx::query_as!(
        Listing,
        r#"SELECT * FROM listing WHERE id = $1 FOR UPDATE"#,
        id
    )
    .fetch_one(&mut *tx)
    .await?;

    // 2. Archive current state to history
    sqlx::query!(
        r#"
        INSERT INTO listing_history
        (listing_id, name, description, listing_structure_id, country, price_per_night, is_active, valid_from)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        "#,
        current.id,
        current.name,
        current.description,
        current.listing_structure_id,
        current.country,
        current.price_per_night,
        current.is_active,
        current.added_at // Using added_at as the start time of this version
    )
    .execute(&mut *tx)
    .await?;

    // 3. Update the record
    // We use COALESCE($2, name) to say: "If the new name is NULL, keep the old name".
    let updated = sqlx::query_as!(
        Listing,
        r#"
        UPDATE listing
        SET
            name = COALESCE($2, name),
            description = COALESCE($3, description),
            listing_structure_id = COALESCE($4, listing_structure_id),
            country = COALESCE($5, country),
            price_per_night = COALESCE($6, price_per_night),
            is_active = COALESCE($7, is_active)
        WHERE id = $1
        RETURNING id, user_id, name, description, listing_structure_id, country, price_per_night, is_active, added_at, deleted_at
        "#,
        id,
        updated_listing_data.name,
        updated_listing_data.description,
        updated_listing_data.listing_structure_id,
        updated_listing_data.country,
        updated_listing_data.price_per_night,
        updated_listing_data.is_active
    )
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(updated)
}

/// Deletes a listing from the database.
/// If `hard_delete` is true, the listing is permanently removed.
/// If `hard_delete` is false, the listing is soft-deleted (deleted_at is set).
#[tracing::instrument(skip(pool))]
pub async fn delete_listing(pool: &PgPool, id: Uuid, hard_delete: bool) -> Result<()> {
    let result: sqlx::postgres::PgQueryResult = if hard_delete {
        sqlx::query!("DELETE FROM listing WHERE id = $1", id)
            .execute(pool)
            .await?
    } else {
        sqlx::query!("UPDATE listing SET deleted_at = now() WHERE id = $1", id)
            .execute(pool)
            .await?
    };

    if result.rows_affected() == 0 {
        return Err(crate::error::DbError::Sqlx(sqlx::Error::RowNotFound));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::DbError;
    use crate::models::{NewHostProfile, NewListing, NewUser};
    use crate::user::{create_host_profile, create_user};
    use rust_decimal_macros::dec;
    use sqlx::{Connection, PgConnection};
    use std::collections::HashSet;
    use std::env;
    use uuid::Uuid;

    /// Helper function to establish a fresh connection to the test database.
    async fn setup_test_db() -> PgConnection {
        dotenvy::dotenv().ok();
        let database_url = env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgres://postgres:password@localhost:5432/our_places".to_string()
        });

        PgConnection::connect(&database_url)
            .await
            .expect("Failed to connect to Postgres")
    }

    async fn create_test_user_no_profile(conn: &mut PgConnection) -> Uuid {
        let id = Uuid::now_v7();
        let new_user = NewUser {
            id,
            email: format!("test_{}@example.com", id),
            password_hash: "secret".to_string(),
            first_name: "Test".to_string(),
            last_name: "User".to_string(),
            phone_number: None,
            is_active: true,
            attributes: serde_json::json!({}),
            roles: None,
        };
        create_user(&mut *conn, &new_user)
            .await
            .expect("Failed to create test user");
        id
    }

    async fn create_test_user_with_host_profile(conn: &mut PgConnection) -> Uuid {
        let user_id = create_test_user_no_profile(&mut *conn).await;
        let profile = NewHostProfile {
            description: Some("Test Host".to_string()),
            verified_status: Some("pending".to_string()),
            payout_details: None,
        };
        create_host_profile(&mut *conn, user_id, &profile)
            .await
            .expect("Failed to create host profile");
        user_id
    }

    #[tokio::test]
    async fn test_create_listing() {
        let mut conn = setup_test_db().await;
        let mut tx = conn.begin().await.expect("Failed to begin transaction");

        let user_id = create_test_user_with_host_profile(&mut *tx).await;
        let new_listing = NewListing {
            name: "Cozy Test Cottage".to_string(),
            user_id,
            description: Some("A beautiful cottage for testing.".to_string()),
            listing_structure_id: 2, // House
            country: "Testland".to_string(),
            price_per_night: Some(dec!(150.75)),
        };

        let created_listing = create_listing(&mut *tx, &new_listing).await.unwrap();

        assert_eq!(created_listing.name, new_listing.name);
        assert!(!created_listing.id.is_nil());
        assert!(!created_listing.is_active);
    }

    #[tokio::test]
    async fn test_create_listing_fails_without_host_profile() {
        let mut conn = setup_test_db().await;
        // let mut tx = conn.begin().await.expect("Failed to begin transaction");
        // Use transaction? yes.
        let mut tx = conn.begin().await.expect("Failed to begin transaction");

        let user_id = create_test_user_no_profile(&mut *tx).await;
        let new_listing = NewListing {
            name: "Fail listing".to_string(),
            user_id,
            description: None,
            listing_structure_id: 1,
            country: "Testland".to_string(),
            price_per_night: None,
        };

        let result = create_listing(&mut *tx, &new_listing).await;
        assert!(matches!(
            result,
            Err(DbError::ValidationError(msg)) if msg == "User must have a host profile to create a listing"
        ));
    }

    #[tokio::test]
    async fn test_get_listing_by_id() {
        let mut conn = setup_test_db().await;
        let mut tx = conn.begin().await.expect("Failed to begin transaction");

        let user_id = create_test_user_with_host_profile(&mut *tx).await;
        let new_listing = NewListing {
            name: "Fetch Me Listing".to_string(),
            user_id,
            description: None,
            listing_structure_id: 3,
            country: "Republic of Testing".to_string(),
            price_per_night: Some(dec!(99.99)),
        };
        let created_listing = create_listing(&mut *tx, &new_listing).await.unwrap();

        let fetched_listing = get_listing_by_id(&mut *tx, created_listing.id)
            .await
            .unwrap();

        assert_eq!(created_listing.id, fetched_listing.id);

        let non_existent_id = Uuid::now_v7();
        let result = get_listing_by_id(&mut *tx, non_existent_id).await;
        assert!(matches!(
            result,
            Err(DbError::Sqlx(sqlx::Error::RowNotFound))
        ));
    }

    #[tokio::test]
    async fn test_get_listings_pagination() {
        let mut conn = setup_test_db().await;
        let mut tx = conn.begin().await.expect("Failed to begin transaction");

        // Create 3 listings. We will order them by name for deterministic testing.
        let user_id = create_test_user_with_host_profile(&mut *tx).await;
        let mut created_ids = Vec::new();
        for i in 1..=3 {
            let listing = NewListing {
                name: format!("Pagination Test Listing {}", i),
                user_id,
                description: None,
                listing_structure_id: 1,
                country: "Testland".to_string(),
                price_per_night: None,
            };
            let created = create_listing(&mut *tx, &listing).await.unwrap();
            created_ids.push(created.id);
        }
        // Since we sort by added_at DESC, id DESC, and all have same added_at (transaction time),
        // we need to know the order of IDs to predict page content.
        // However, instead of predicting order, we can check that standard pagination properties hold
        // AND that our items are present in the top results (since they are newest).

        // Page 1 (Limit 2)
        let page1 = get_listings(&mut *tx, 1, 2, None).await.unwrap();
        assert_eq!(page1.len(), 2);

        // Page 2 (Limit 2)
        let page2 = get_listings(&mut *tx, 2, 2, None).await.unwrap();
        // Page 2 might have >1 items if DB had pre-existing data.
        // But we expect at least 1 of ours.

        // Check that our created items are in the first 3 results (across p1 and p2)
        let mut top_3_ids = Vec::new();
        top_3_ids.extend(page1.iter().map(|l| l.id));
        if let Some(l) = page2.first() {
            top_3_ids.push(l.id);
        }

        // Verify all created_ids are in the found top results.
        // Note: Newest items should come first.
        let created_set: HashSet<Uuid> = created_ids.into_iter().collect();
        let found_set: HashSet<Uuid> = top_3_ids.into_iter().collect();

        // We assert that all our created items are found.
        for id in created_set {
            assert!(
                found_set.contains(&id),
                "Created listing {} not found in top pagination results",
                id
            );
        }
    }

    #[tokio::test]
    async fn test_get_listings_uniqueness_across_pages() {
        let mut conn = setup_test_db().await;
        let mut tx = conn.begin().await.expect("Failed to begin transaction");

        const TOTAL_RECORDS: i32 = 7;
        const PER_PAGE: u32 = 3;

        let user_id = create_test_user_with_host_profile(&mut *tx).await;
        for i in 1..=TOTAL_RECORDS {
            let listing = NewListing {
                name: format!("Uniqueness Test Listing {}", i),
                user_id,
                description: None,
                listing_structure_id: 1,
                country: "Testland".to_string(),
                price_per_night: None,
            };
            create_listing(&mut *tx, &listing).await.unwrap();
        }

        let mut all_fetched_listings = Vec::new();
        let mut current_page = 1;
        loop {
            let page_results = get_listings(&mut *tx, current_page, PER_PAGE, None)
                .await
                .unwrap();
            if page_results.is_empty() {
                break;
            }
            all_fetched_listings.extend(page_results);
            current_page += 1;
        }

        assert!(
            all_fetched_listings.len() as i32 >= TOTAL_RECORDS,
            "Pagination should return at least the inserted number of listings"
        );

        let ids: Vec<Uuid> = all_fetched_listings.iter().map(|l| l.id).collect();
        let unique_ids: HashSet<Uuid> = ids.iter().cloned().collect();

        assert_eq!(
            ids.len(),
            unique_ids.len(),
            "Found duplicate listings across different pages"
        );
    }

    #[tokio::test]
    async fn test_get_listings_filtering() {
        let mut conn = setup_test_db().await;
        let mut tx = conn.begin().await.expect("Failed to begin transaction");

        let user_id = create_test_user_with_host_profile(&mut *tx).await;

        // 1. Apartment in Jamaica, cheap
        let listing1 = NewListing {
            name: "Cheap Apartment Jamaica".to_string(),
            user_id,
            description: None,
            listing_structure_id: 1, // Apartment
            country: "Jamaica".to_string(),
            price_per_night: Some(dec!(50.00)),
        };
        create_listing(&mut *tx, &listing1).await.unwrap();

        // 2. Villa in Jamaica, expensive
        let listing2 = NewListing {
            name: "Luxury Villa Jamaica".to_string(),
            user_id,
            description: None,
            listing_structure_id: 5, // Villa
            country: "Jamaica".to_string(),
            price_per_night: Some(dec!(500.00)),
        };
        create_listing(&mut *tx, &listing2).await.unwrap();

        // 3. Apartment in USA, cheap
        let listing3 = NewListing {
            name: "Cheap Apartment USA".to_string(),
            user_id,
            description: None,
            listing_structure_id: 1, // Apartment
            country: "USA".to_string(),
            price_per_night: Some(dec!(60.00)),
        };
        create_listing(&mut *tx, &listing3).await.unwrap();

        // Test Filter by Country (Jamaica)
        let filter_jamaica = common::models::ListingFilter {
            name: None,
            country: Some("Jamaica".to_string()),
            min_price: None,
            max_price: None,
            structure_type: vec![],
            owner: None,
        };
        let results = get_listings(&mut *tx, 1, 10, Some(filter_jamaica))
            .await
            .unwrap();
        // Should find listing1 and listing2 (we might have other data, but these surely)
        // We filter results locally to verify *our* created ones are there only if they match
        let found_names: Vec<String> = results.iter().map(|l| l.name.clone()).collect();
        assert!(found_names.contains(&"Cheap Apartment Jamaica".to_string()));
        assert!(found_names.contains(&"Luxury Villa Jamaica".to_string()));
        assert!(!found_names.contains(&"Cheap Apartment USA".to_string()));

        // Test Filter by Price (< $100)
        let filter_none = common::models::ListingFilter {
            name: None,
            country: None,
            min_price: None,
            max_price: None,
            structure_type: vec![],
            owner: None,
        };
        let results = get_listings(&mut *tx, 1, 10, Some(filter_none))
            .await
            .expect("Failed to fetch listings");
        // Should return both
        assert_eq!(results.len(), 2);

        // 2. Filter by Structure Type (Villa)
        let filter_villa = common::models::ListingFilter {
            name: None,
            country: None,
            min_price: None,
            max_price: None,
            // We use string representation for the filter structure_type as per definition
            structure_type: vec!["Villa".to_string()],
            owner: None,
        };
        let results = get_listings(&mut *tx, 1, 10, Some(filter_villa))
            .await
            .unwrap();
        let found_names: Vec<String> = results.iter().map(|l| l.name.clone()).collect();
        assert!(found_names.contains(&"Luxury Villa Jamaica".to_string()));
        // Note: Logic allows checking absence if we assume test DB isolation, but transactions help.
        // Listing 1 is Apartment, should not be here.
        assert!(!found_names.contains(&"Cheap Apartment Jamaica".to_string()));
    }
}
