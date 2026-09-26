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
            let place = details(item)
            let coordinate = place.location.coordinate
            var out: [String: Any] = [
                "name": item.name ?? place.city ?? query,
                "latitude": coordinate.latitude,
                "longitude": coordinate.longitude,
            ]
            out["address"] = place.address
            out["city"] = place.city
            out["region"] = place.region
            out["country"] = place.country
            out["countryCode"] = place.countryCode
            out["mapIdentifier"] = item.identifier?.rawValue
            // Lets the ranking tell "near the place the screenshot talks about" even when Maps omits the country.
            out["nearDistanceKm"] = center.map { place.location.distance(from: $0) / 1000 }
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
        let region = MKCoordinateRegion(center: details(item).location.coordinate, span: MKCoordinateSpan(latitudeDelta: span, longitudeDelta: span))
        await cache.set(near, region)
        return region
    }
}

extension MapsService {
    struct Details {
        var location: CLLocation
        var address: String?
        var city: String?
        var region: String?
        var country: String?
        var countryCode: String?
    }

    /// Location and address of a result. macOS 26 has `location`/`address`/`addressRepresentations`;
    /// earlier versions use the (since-deprecated) `placemark`, which carries the same information.
    static func details(_ item: MKMapItem) -> Details {
        if #available(macOS 26.0, *) {
            let reps = item.addressRepresentations
            return Details(location: item.location, address: item.address?.shortAddress ?? item.address?.fullAddress,
                           city: reps?.cityName, region: reps?.cityWithContext, country: reps?.regionName,
                           countryCode: reps?.region?.identifier)
        }
        let p = item.placemark
        let street = [p.subThoroughfare, p.thoroughfare].compactMap { $0 }.joined(separator: " ")
        let address = [street.isEmpty ? nil : street, p.locality].compactMap { $0 }.joined(separator: ", ")
        return Details(location: p.location ?? CLLocation(latitude: p.coordinate.latitude, longitude: p.coordinate.longitude),
                       address: address.isEmpty ? nil : address, city: p.locality,
                       region: [p.locality, p.administrativeArea, p.country].compactMap { $0 }.joined(separator: ", "),
                       country: p.country, countryCode: p.isoCountryCode)
    }
}
