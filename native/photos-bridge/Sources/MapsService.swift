import Foundation
import MapKit

/// Apple Maps place search. Returns raw candidates only; ranking happens in Rust.
enum MapsService {
    /// Regions already looked up for "near" hints (e.g. "Nanjing, China").
    private actor RegionCache {
        var regions: [String: MKCoordinateRegion] = [:]
        func get(_ key: String) -> MKCoordinateRegion? { regions[key] }
        func set(_ key: String, _ value: MKCoordinateRegion) { regions[key] = value }
    }
    private static let cache = RegionCache()

    /// Searches around `near` when given, so results aren't biased toward the user's own location.
    static func search(query: String, limit: Int, near: String?) async throws -> [[String: Any]] {
        let request = MKLocalSearch.Request()
        request.naturalLanguageQuery = query
        request.resultTypes = [.pointOfInterest, .address, .physicalFeature]
        var center: CLLocation?
        if let near, !near.isEmpty, let region = try? await region(for: near) {
            request.region = region
            center = CLLocation(latitude: region.center.latitude, longitude: region.center.longitude)
        }

        let response: MKLocalSearch.Response
        do {
            response = try await MKLocalSearch(request: request).start()
        } catch let error as MKError where error.code == .placemarkNotFound {
            return []
        } catch let error as MKError where error.code == .loadingThrottled {
            throw BridgeError.message("throttled")
        }

        return response.mapItems.prefix(limit).map { item in
            let reps = item.addressRepresentations
            let coordinate = item.location.coordinate
            var out: [String: Any] = [
                "name": item.name ?? reps?.cityName ?? query,
                "latitude": coordinate.latitude,
                "longitude": coordinate.longitude,
            ]
            out["address"] = item.address?.shortAddress ?? item.address?.fullAddress
            out["city"] = reps?.cityName
            out["region"] = reps?.cityWithContext
            out["country"] = reps?.regionName
            out["countryCode"] = reps?.region?.identifier
            out["mapIdentifier"] = item.identifier?.rawValue
            // Lets the ranking tell "near the place the screenshot talks about" even when Maps omits the country.
            out["nearDistanceKm"] = center.map { item.location.distance(from: $0) / 1000 }
            out["category"] = item.pointOfInterestCategory?.rawValue.replacingOccurrences(of: "MKPOICategory", with: "")
            return out.compactMapValues { $0 }
        }
    }

    /// Region for a place hint: ~100 km around a city, ~1,500 km around a country.
    private static func region(for near: String) async throws -> MKCoordinateRegion? {
        if let cached = await cache.get(near) { return cached }
        let request = MKLocalSearch.Request()
        request.naturalLanguageQuery = near
        request.resultTypes = [.address]
        guard let item = try await MKLocalSearch(request: request).start().mapItems.first else { return nil }
        let isCity = near.contains(",")
        let span = isCity ? 1.0 : 15.0
        let region = MKCoordinateRegion(center: item.location.coordinate, span: MKCoordinateSpan(latitudeDelta: span, longitudeDelta: span))
        await cache.set(near, region)
        return region
    }
}
